use super::*;

fn operator_file_reference(value: &str) -> Result<PathBuf> {
    if value.starts_with("file://") {
        let path = value
            .strip_prefix("file://")
            .expect("checked file URI prefix");
        if path.is_empty()
            || path.contains('?')
            || path.contains('#')
            || path.contains('@')
            || !path.starts_with('/')
        {
            bail!("upgrade operator_file reference must be a credential-free canonical file URI");
        }
        return Ok(PathBuf::from(path));
    }
    if value.contains("://") {
        bail!("upgrade source_kind is operator_file in v0.2; network sources are closed");
    }
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        bail!("upgrade operator_file path must be absolute");
    }
    Ok(path)
}

pub(super) fn run_upgrade(mut args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        bail!("upgrade requires <artifact-ref>");
    }
    let artifact_ref = args.remove(0);
    let _source = operator_file_reference(&artifact_ref)?;
    let _root = take_option(&mut args, "--root").map(PathBuf::from);
    let _expected_digest = take_option(&mut args, "--expected-digest");
    let _approval = take_option(&mut args, "--approve-artifact");
    let _json = take_flag(&mut args, "--json");
    if take_flag(&mut args, "--yes") {
        bail!("upgrade has no broad --yes authority; approve the exact release artifact digest");
    }
    if !args.is_empty() {
        bail!("unexpected upgrade arguments: {}", args.join(" "));
    }
    bail!("upgrade release acquisition is not yet implemented")
}
