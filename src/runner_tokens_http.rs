//! Runner enrollment-token management. These operational routes are not model
//! tools and retain their account/bootstrap authorization requirements.

#[cfg(test)]
use crate::auth::AGENT_SCOPES;
#[cfg(test)]
use crate::models::ApiKeyRecord;
#[cfg(test)]
use salvo::prelude::*;
#[cfg(test)]
use serde_json::{json, Value};

mod responses;
mod routes;

#[cfg(test)]
use responses::agent_token_summary;
pub(crate) use routes::{
    runner_tokens_create, runner_tokens_list, runner_tokens_register_hash, runner_tokens_revoke,
};

#[cfg(test)]
mod tests;
