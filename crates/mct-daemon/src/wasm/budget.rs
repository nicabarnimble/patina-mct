use super::*;
use crate::resource_bounds::serialized_size;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MctWasmInvocationLimits {
    pub host_calls: usize,
    pub logging_calls: usize,
    pub measure_calls: usize,
    pub toy_calls: usize,
    pub observations: usize,
    /// Conservative cumulative charge: observations, host inputs/outputs and messages.
    pub accumulated_bytes: usize,
    pub host_value_bytes: usize,
    pub output_bytes: usize,
}

impl Default for MctWasmInvocationLimits {
    fn default() -> Self {
        Self {
            host_calls: 4096,
            logging_calls: 256,
            measure_calls: 1024,
            toy_calls: 1024,
            observations: 2050,
            accumulated_bytes: 4 * 1024 * 1024,
            host_value_bytes: 64 * 1024,
            output_bytes: 256 * 1024,
        }
    }
}

#[derive(Debug, Error)]
#[error("WASM invocation budget exceeded: {0}")]
pub(super) struct BudgetExceeded(pub &'static str);

pub(super) struct InvocationBudget {
    pub limits: MctWasmInvocationLimits,
    host_calls: usize,
    logging: usize,
    measure: usize,
    toys: usize,
    observations: usize,
    bytes: usize,
    pub failed: Option<&'static str>,
}

impl InvocationBudget {
    pub fn new(limits: MctWasmInvocationLimits) -> Self {
        Self {
            limits,
            host_calls: 0,
            logging: 0,
            measure: 0,
            toys: 0,
            observations: 0,
            bytes: 0,
            failed: None,
        }
    }
    pub fn fail<T>(&mut self, resource: &'static str) -> wasmtime::Result<T> {
        let first = *self.failed.get_or_insert(resource);
        Err(BudgetExceeded(first).into())
    }
    pub fn host_call(&mut self) -> wasmtime::Result<()> {
        if self.failed.is_some() || self.host_calls >= self.limits.host_calls {
            return self.fail("host calls");
        }
        self.host_calls += 1;
        Ok(())
    }
    pub fn logging_call(&mut self) -> wasmtime::Result<()> {
        if self.failed.is_some() || self.logging >= self.limits.logging_calls {
            return self.fail("logging calls");
        }
        self.logging += 1;
        Ok(())
    }
    pub fn measure_call(&mut self) -> wasmtime::Result<()> {
        if self.failed.is_some() || self.measure >= self.limits.measure_calls {
            return self.fail("measure calls");
        }
        self.measure += 1;
        Ok(())
    }
    pub fn bytes(&mut self, size: usize, resource: &'static str) -> wasmtime::Result<()> {
        if self.failed.is_some() || size > self.limits.accumulated_bytes.saturating_sub(self.bytes)
        {
            return self.fail(resource);
        }
        self.bytes += size;
        Ok(())
    }
    pub fn observations(&mut self, count: usize, size: usize) -> wasmtime::Result<()> {
        if count > self.limits.observations.saturating_sub(self.observations) {
            return self.fail("observations");
        }
        self.bytes(size, "observation bytes")?;
        self.observations += count;
        Ok(())
    }
    pub fn next_toy_index(&self) -> usize {
        self.toys.saturating_add(1)
    }
    pub fn toy(&mut self, observation_bytes: usize) -> wasmtime::Result<()> {
        if self.toys >= self.limits.toy_calls {
            return self.fail("toy calls");
        }
        self.observations(2, observation_bytes)?;
        self.toys += 1;
        Ok(())
    }
    pub fn json_bytes(
        &mut self,
        value: &impl Serialize,
        limit: usize,
        resource: &'static str,
    ) -> wasmtime::Result<usize> {
        let Ok(size) = serialized_size(value, limit) else {
            return self.fail(resource);
        };
        self.bytes(size, resource)?;
        Ok(size)
    }
    pub fn host_values(&mut self, values: &[component::Val]) -> wasmtime::Result<()> {
        let Some(size) = component_values_size(values, self.limits.host_value_bytes) else {
            return self.fail("host argument bytes");
        };
        self.bytes(size, "host argument bytes")
    }
}

/// Charge dynamic Val container overhead as well as payload bytes before copying
/// into JSON or other host collections. Wasmtime owns the preceding ABI lift.
pub(super) fn component_values_size(values: &[component::Val], limit: usize) -> Option<usize> {
    fn visit(value: &component::Val, left: &mut usize) -> Option<()> {
        *left = left.checked_sub(std::mem::size_of::<component::Val>())?;
        use component::Val::*;
        match value {
            String(s) | Enum(s) => {
                *left = left.checked_sub(s.len())?;
            }
            List(values) | Tuple(values) => {
                for value in values {
                    visit(value, left)?;
                }
            }
            Record(fields) => {
                for (key, value) in fields {
                    *left =
                        left.checked_sub(key.len() + std::mem::size_of::<std::string::String>())?;
                    visit(value, left)?;
                }
            }
            Flags(names) => {
                for name in names {
                    *left =
                        left.checked_sub(name.len() + std::mem::size_of::<std::string::String>())?;
                }
            }
            Variant(name, value) => {
                *left = left.checked_sub(name.len())?;
                if let Some(value) = value {
                    visit(value, left)?;
                }
            }
            Option(Some(value)) | Result(Ok(Some(value))) | Result(Err(Some(value))) => {
                visit(value, left)?
            }
            _ => {}
        }
        Some(())
    }
    let mut left = limit;
    for value in values {
        visit(value, &mut left)?;
    }
    Some(limit - left)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_byte_count_and_sticky_overflow_never_reset_within_invocation() {
        let mut budget = InvocationBudget::new(MctWasmInvocationLimits {
            accumulated_bytes: 10,
            observations: 2,
            ..Default::default()
        });
        budget.observations(2, 10).unwrap();
        assert!(budget.bytes(1, "output bytes").is_err());
        assert!(budget.bytes(0, "output bytes").is_err());
        assert!(budget.host_call().is_err());
        assert_eq!(budget.bytes, 10);
        assert_eq!(budget.observations, 2);
        assert_eq!(budget.failed, Some("output bytes"));
        let fresh = InvocationBudget::new(MctWasmInvocationLimits::default());
        assert_eq!(fresh.failed, None);
    }
    #[test]
    fn escaped_json_and_nested_values_are_charged_without_unbounded_serialization() {
        assert_eq!(serialized_size(&"\n", 4).unwrap(), 4);
        assert!(serialized_size(&"\n", 3).is_err());
        let value = component::Val::List(vec![component::Val::String("x".repeat(128))]);
        let values = std::slice::from_ref(&value);
        assert!(component_values_size(values, 128).is_none());
        let exact = component_values_size(values, 1024).unwrap();
        assert_eq!(component_values_size(values, exact), Some(exact));
        assert!(component_values_size(values, exact - 1).is_none());
    }
}
