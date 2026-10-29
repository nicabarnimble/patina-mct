//! Explicit administrative ALPN. Connectivity never supplies grant authority.
use super::*;
use mct_kernel::admin_key::PasskeyAdminActionV1;
use mct_kernel::admin_sig::PasskeyAdminRequestV1;

pub const MCT_ADMIN_ALPN: &str = "mct/admin/1";
pub const MCT_ADMIN_FRAME_MAX_BYTES: usize = 96 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum MctIrohAdminReply {
    Completed {
        result: serde_json::Value,
    },
    Rejected,
    Unavailable,
    /// No safe automatic retry: a canonical fact/effect may have committed.
    Indeterminate,
}
type AdminFuture = Pin<Box<dyn Future<Output = MctIrohAdminReply> + Send + 'static>>;
/// Trusted Mother-owned verifier/dispatcher, not a transport authentication token.
/// It must verify canonical provenance, signature, scope, clock and nonce, and
/// await required durability before performing an effect. No EndpointId or UID
/// is supplied as a possible substitute for that proof.
#[derive(Clone)]
pub struct MctIrohAdminHandler(Arc<dyn Fn(PasskeyAdminRequestV1) -> AdminFuture + Send + Sync>);
impl fmt::Debug for MctIrohAdminHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MctIrohAdminHandler")
    }
}
impl MctIrohAdminHandler {
    pub fn new<F, Fut>(handler: F) -> Self
    where
        F: Fn(PasskeyAdminRequestV1) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = MctIrohAdminReply> + Send + 'static,
    {
        Self(Arc::new(move |request| Box::pin(handler(request))))
    }
}
pub(super) async fn dispatch(
    handler: Option<&MctIrohAdminHandler>,
    bytes: &[u8],
    now: Timestamp,
) -> MctIrohAdminReply {
    if bytes.len() > MCT_ADMIN_FRAME_MAX_BYTES {
        return MctIrohAdminReply::Rejected;
    }
    let Ok(request) = serde_json::from_slice::<PasskeyAdminRequestV1>(bytes) else {
        return MctIrohAdminReply::Rejected;
    };
    if request.intent.validate(&now).is_err()
        || matches!(
            request.intent.action,
            PasskeyAdminActionV1::Bootstrap { .. }
        )
    {
        return MctIrohAdminReply::Rejected;
    }
    let Some(handler) = handler else {
        return MctIrohAdminReply::Unavailable;
    };
    // Dropping a socket/transport task must not cancel a durability callback.
    tokio::spawn((handler.0)(request))
        .await
        .unwrap_or(MctIrohAdminReply::Indeterminate)
}
pub(super) fn encode_reply(reply: &MctIrohAdminReply) -> MotherIrohEndpointResult<Vec<u8>> {
    let bytes =
        serde_json::to_vec(reply).map_err(|source| MotherIrohEndpointError::ProtocolJson {
            action: "encode admin reply",
            source,
        })?;
    if bytes.len() > MCT_ADMIN_FRAME_MAX_BYTES {
        return Ok(br#"{"status":"indeterminate"}"#.to_vec());
    }
    Ok(bytes)
}
impl MotherIrohEndpoint {
    pub async fn send_admin(
        &self,
        peer: &MotherIrohEndpointTicket,
        request: &PasskeyAdminRequestV1,
    ) -> MotherIrohEndpointResult<MctIrohAdminReply> {
        let bytes = serde_json::to_vec(request).map_err(|source| {
            MotherIrohEndpointError::ProtocolJson {
                action: "encode admin request",
                source,
            }
        })?;
        if bytes.len() > MCT_ADMIN_FRAME_MAX_BYTES {
            return Err(MotherIrohEndpointError::ProtocolIo {
                action: "bound admin request",
                source: boxed_source(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "admin request exceeds byte cap",
                )),
            });
        }
        self.roundtrip_json(peer, MCT_ADMIN_ALPN, request).await
    }
}
impl crate::MotherIrohClient {
    pub async fn send_admin(
        &self,
        peer: &MotherIrohEndpointTicket,
        request: &PasskeyAdminRequestV1,
    ) -> MotherIrohEndpointResult<MctIrohAdminReply> {
        self.inner.send_admin(peer, request).await
    }
}
