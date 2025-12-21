use anyhow::Result;
use rsip::{Response, StatusCodeKind};
use rsipstack::{
    dialog::{authenticate::Credential, registration::Registration},
    rsip_ext::RsipResponseExt,
    transaction::endpoint::EndpointInnerRef,
};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Instant};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct UserCredential {
    pub username: String,
    pub password: String,
    pub realm: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct RegisterOption {
    pub server: String,
    pub username: String,
    pub display_name: Option<String>,
    pub disabled: Option<bool>,
    pub credential: Option<UserCredential>,
}

impl From<UserCredential> for Credential {
    fn from(val: UserCredential) -> Self {
        Credential {
            username: val.username,
            password: val.password,
            realm: val.realm,
        }
    }
}

impl RegisterOption {
    pub fn aor(&self) -> String {
        format!("{}@{}", self.username, self.server)
    }
}

pub struct RegistrationHandleInner {
    pub endpoint_inner: EndpointInnerRef,
    pub option: RegisterOption,
    pub cancel_token: CancellationToken,
    pub start_time: Mutex<Instant>,
    pub last_update: Mutex<Instant>,
    pub last_response: Mutex<Option<Response>>,
}
#[derive(Clone)]
pub struct RegistrationHandle {
    pub inner: Arc<RegistrationHandleInner>,
}

impl RegistrationHandle {
    pub fn stop(&self) {
        self.inner.cancel_token.cancel();
    }

    pub async fn do_register(&self, sip_server: &rsip::Uri, expires: Option<u32>) -> Result<u32> {
        let mut registration = Registration::new(
            self.inner.endpoint_inner.clone(),
            self.inner.option.credential.clone().map(|c| c.into()),
        );
        let resp = match registration
            .register(sip_server.clone(), expires)
            .await
            .map_err(|e| anyhow::anyhow!("Registration failed: {}", e))
        {
            Ok(resp) => resp,
            Err(e) => {
                warn!("registration failed: {}", e);
                return Err(anyhow::anyhow!("Registration failed: {}", e));
            }
        };

        debug!(
            user = self.inner.option.aor(),
            "registration response: {:?}", resp
        );
        match resp.status_code().kind() {
            StatusCodeKind::Successful => {
                *self.inner.last_update.lock().await = Instant::now();
                *self.inner.last_response.lock().await = Some(resp);
                Ok(registration.expires())
            }
            _ => Err(anyhow::anyhow!("{:?}", resp.reason_phrase())),
        }
    }
}
