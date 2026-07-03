use super::*;
use crate::auth::{
    generate_account_credential, generate_agent_token, generate_api_token,
    generate_oauth_authorization_code, hash_token, shared_key_hash_of, token_prefix, AuthKind,
    OAuth2Verifier, TokenVerifier,
};
use crate::models::{
    AccountCredentialRecord, ApiKeyRecord, OAuthAuthorizationCodeRecord, OAuthClientRecord,
    UserRecord, TOKEN_KIND_AGENT, TOKEN_KIND_USER,
};
use crate::OAuth2Config;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use salvo::prelude::*;
use salvo::test::{ResponseExt, TestClient};
use salvo::Service;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::Arc;

mod authorize;
mod clients;
mod managed_authorize;
mod metadata;
mod revoke;
mod scopes;
mod shared_key_bridge;
mod support;
mod token;

use support::*;
