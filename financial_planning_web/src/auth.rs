use std::collections::HashMap;
use std::path::Path;

use actix_web_httpauth::extractors::AuthenticationError;
use actix_web::dev::ServiceRequest;
use actix_web_httpauth::extractors::basic::{BasicAuth, Config};

use anyhow::{Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(transparent)]
pub struct User(String);

#[derive(Clone)]
struct Password(String);

#[derive(Clone, Debug)]
pub struct Salt(String);

#[derive(Clone, Debug)]
pub struct Hash(String);

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    users: HashMap<User, String>,
}

#[derive(Clone)]
pub struct AuthProvider {
    hashes: HashMap<User, (Salt, Hash)>,
}

impl AuthProvider {
    pub fn new(hashes: HashMap<User, (Salt, Hash)>) -> Self {
        Self { hashes }
    }

    pub fn new_from_file(path: &Path) -> Result<Self> {
        let config: ConfigFile = toml::from_str(
            &std::fs::read_to_string(&path)
                .context(format!("Failed to read {:?} file contents", path))?,
        )
        .context("Failed to parse auth config")?;

        let mut hashes = HashMap::new();
        for (user, salt_hash) in config.users.into_iter() {
            let (salt, hash) = salt_hash.split_once(':').context(format!("Failed to find : for auth entry {}", user.0))?;
            hashes.insert(user, (Salt(salt.to_string()), Hash(hash.to_string())));
        }
        Ok(Self::new(hashes))
    }

    fn check_hash(&self, user: User, pw: Password) -> bool {
        let mut hasher = Sha256::new();

        match self.hashes.get(&user) {
            Some((salt, hash)) => {
                hasher.update(&salt.0);
                hasher.update(&pw.0);
                let result = hasher.finalize();
                let hex_str = format!("{:x}", result);
                if hex_str == hash.0 {
                    println!("Accepting user {:?}", user.0);
                    true
                } else {
                    println!("Rejecting user {:?}", user.0);
                    false
                }
            },
            None => {
                println!("Rejecting unknown user: {}", user.0);
                false
            }
        }
    }

    pub fn validate_request(
        &self,
        req: ServiceRequest,
        credentials: BasicAuth,
    ) -> Result<ServiceRequest, actix_web::Error> {
        let config = req
            .app_data::<Config>()
            .map(|data| data.as_ref().clone())
            .unwrap_or_else(Default::default);

        let user_id = credentials.user_id().to_owned().to_string();
        let pw = match credentials.password() {
            Some(pw) => Password(pw.to_owned().to_string()),
            None => {
                return Err(AuthenticationError::new(config).into());
            }
        };

        if self.check_hash(User(user_id), pw) {
            Ok(req)
        } else {
            Err(AuthenticationError::new(config).into())
        }

    }
}
