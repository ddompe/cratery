/*******************************************************************************
 * Copyright (c) 2024 Cénotélie Opérations SAS (cenotelie.fr)
 ******************************************************************************/

//! Module for configuration management

use std::error::Error;
use std::net::{IpAddr, Ipv4Addr};
use std::str::FromStr;

use axum::http::Uri;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use cenotelie_lib_apierror::ApiError;
use cenotelie_lib_s3::S3Params;
use serde_derive::{Deserialize, Serialize};
use tokio::fs::File;
use tokio::io::{AsyncWriteExt, BufWriter};

use super::errors::MissingEnvVar;

/// Gets the value for an environment variable
pub fn get_var<T: AsRef<str>>(name: T) -> Result<String, MissingEnvVar> {
    let key = name.as_ref();
    std::env::var(key).map_err(|original| MissingEnvVar {
        original,
        var_name: key.to_string(),
    })
}

/// the configuration for an external registry
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConfigExternalRegistry {
    /// The name for the registry
    pub name: String,
    /// The URI to the registry's index
    pub index: String,
    /// The root uri to docs for packages in this registry
    #[serde(rename = "docsRoot")]
    pub docs_root: String,
    /// The login to connect to the registry
    pub login: String,
    /// The token for authentication
    pub token: String,
}

/// A configuration for the registry
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Configuration {
    /// The log level to use
    #[serde(rename = "logLevel")]
    pub log_level: String,
    /// The datetime format to use when logging
    #[serde(rename = "logDatetimeFormat")]
    pub log_datetime_format: String,
    /// The IP to bind for the web server
    #[serde(rename = "webListenOnIp")]
    pub web_listenon_ip: IpAddr,
    /// The port to bind for the web server
    #[serde(rename = "webListenOnPort")]
    pub web_listenon_port: u16,
    /// The root uri from which the application is served
    #[serde(rename = "webPublicUri")]
    pub web_public_uri: String,
    /// The domain for the application
    #[serde(rename = "webDomain")]
    pub web_domain: String,
    /// The maximum size for the body of incoming requests
    #[serde(rename = "webBodyLimit")]
    pub web_body_limit: usize,
    /// The data directory
    #[serde(rename = "dataDir")]
    pub data_dir: String,
    /// The configuration for the index
    #[serde(rename = "indexConfig")]
    pub index_config: IndexConfig,
    /// The parameters to connect to S3
    pub s3: S3Params,
    /// The name of the s3 bucket to use
    pub bucket: String,
    /// The uri of the OAuth login page
    #[serde(rename = "oauthLoginUri")]
    pub oauth_login_uri: String,
    /// The uri of the OAuth token API endpoint
    #[serde(rename = "oauthTokenUri")]
    pub oauth_token_uri: String,
    /// The uri of the OAuth userinfo API endpoint
    #[serde(rename = "oauthCallbackUri")]
    pub oauth_callback_uri: String,
    /// The uri of the OAuth userinfo API endpoint
    #[serde(rename = "oauthUserInfoUri")]
    pub oauth_userinfo_uri: String,
    /// The identifier of the client to use
    #[serde(rename = "oauthClientId")]
    pub oauth_client_id: String,
    /// The secret for the client to use
    #[serde(rename = "oauthClientSecret")]
    pub oauth_client_secret: String,
    /// The secret for the client to use
    #[serde(rename = "oauthClientScope")]
    pub oauth_client_scope: String,
    /// The known external registries that require authentication
    #[serde(rename = "externalRegistries")]
    pub external_registries: Vec<ConfigExternalRegistry>,
    /// The login to the service account for self authentication
    #[serde(rename = "selfServiceLogin")]
    pub self_service_login: String,
    /// The token to the service account for self authentication
    #[serde(rename = "selfServiceToken")]
    pub self_service_token: String,
}

impl Configuration {
    /// Gets the configuration from environment variables
    ///
    /// # Errors
    ///
    /// Return a `VarError` when an expected environment variable is not present
    pub fn from_env() -> Result<Self, MissingEnvVar> {
        let data_dir = get_var("REGISTRY_DATA_DIR")?;
        let web_public_uri = get_var("REGISTRY_WEB_PUBLIC_URI")?;
        let index_config = IndexConfig {
            location: format!("{data_dir}/index"),
            remote_origin: get_var("REGISTRY_GIT_REMOTE").ok(),
            remote_ssh_key_file_name: get_var("REGISTRY_GIT_REMOTE_SSH_KEY_FILENAME").ok(),
            remote_push_changes: get_var("REGISTRY_GIT_REMOTE_PUSH_CHANGES")
                .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
                .unwrap_or_default(),
            user_name: get_var("REGISTRY_GIT_USER_NAME")?,
            user_email: get_var("REGISTRY_GIT_USER_EMAIL")?,
            public: IndexPublicConfig {
                dl: format!("{web_public_uri}/api/v1/crates"),
                api: web_public_uri.clone(),
                auth_required: true,
            },
        };
        let mut external_registries = Vec::new();
        let mut external_registry_index = 1;
        while let Ok(name) = get_var(format!("REGISTRY_EXTERNAL_{external_registry_index}_NAME")) {
            let index = get_var(format!("REGISTRY_EXTERNAL_{external_registry_index}_INDEX"))?;
            let docs_root = get_var(format!("REGISTRY_EXTERNAL_{external_registry_index}_DOCS"))?;
            let login = get_var(format!("REGISTRY_EXTERNAL_{external_registry_index}_LOGIN"))?;
            let token = get_var(format!("REGISTRY_EXTERNAL_{external_registry_index}_TOKEN"))?;
            external_registries.push(ConfigExternalRegistry {
                name,
                index,
                docs_root,
                login,
                token,
            });
            external_registry_index += 1;
        }
        Ok(Self {
            log_level: get_var("REGISTRY_LOG_LEVEL").unwrap_or_else(|_| String::from("INFO")),
            log_datetime_format: get_var("REGISTRY_LOG_DATE_TIME_FORMAT")
                .unwrap_or_else(|_| String::from("[%Y-%m-%d %H:%M:%S]")),
            web_listenon_ip: get_var("REGISTRY_WEB_LISTENON_IP")
                .map(|s| IpAddr::from_str(&s).expect("invalud REGISTRY_WEB_LISTENON_IP"))
                .unwrap_or_else(|_| IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))),
            web_listenon_port: get_var("REGISTRY_WEB_LISTENON_PORT")
                .map(|s| s.parse().expect("invalid REGISTRY_WEB_LISTENON_PORT"))
                .unwrap_or(8080),
            web_domain: Uri::from_str(&web_public_uri)
                .expect("invalid REGISTRY_WEB_PUBLIC_URI")
                .host()
                .unwrap_or_default()
                .to_string(),
            web_public_uri,
            web_body_limit: get_var("REGISTRY_WEB_BODY_LIMIT")
                .map_err::<Box<dyn Error>, _>(std::convert::Into::into)
                .and_then(|var| var.parse::<usize>().map_err::<Box<dyn Error>, _>(std::convert::Into::into))
                .unwrap_or(10 * 1024 * 1024),
            data_dir,
            index_config,
            s3: S3Params {
                uri: get_var("REGISTRY_S3_URI")?,
                region: get_var("REGISTRY_S3_REGION")?,
                service: get_var("REGISTRY_S3_SERVICE").ok(),
                access_key: get_var("REGISTRY_S3_ACCESS_KEY")?,
                secret_key: get_var("REGISTRY_S3_SECRET_KEY")?,
            },
            bucket: get_var("REGISTRY_S3_BUCKET")?,
            oauth_login_uri: get_var("REGISTRY_OAUTH_LOGIN_URI")?,
            oauth_token_uri: get_var("REGISTRY_OAUTH_TOKEN_URI")?,
            oauth_callback_uri: get_var("REGISTRY_OAUTH_CALLBACK_URI")?,
            oauth_userinfo_uri: get_var("REGISTRY_OAUTH_USERINFO_URI")?,
            oauth_client_id: get_var("REGISTRY_OAUTH_CLIENT_ID")?,
            oauth_client_secret: get_var("REGISTRY_OAUTH_CLIENT_SECRET")?,
            oauth_client_scope: get_var("REGISTRY_OAUTH_CLIENT_SCOPE")?,
            external_registries,
            self_service_login: super::generate_token(16),
            self_service_token: super::generate_token(64),
        })
    }

    /// Gets the corresponding database url
    pub fn get_database_url(&self) -> String {
        format!("sqlite://{}/registry.db", self.data_dir)
    }

    /// Gets the corresponding index git config
    pub fn get_index_git_config(&self) -> IndexConfig {
        self.index_config.clone()
    }

    /// Write the configuration for authenticating to registries
    ///
    /// # Errors
    ///
    /// Return an error when writing fail
    pub async fn write_auth_config(&self) -> Result<(), ApiError> {
        {
            let file = File::create("/home/cratery/.gitconfig").await?;
            let mut writer = BufWriter::new(file);
            writer.write_all("[credential]\n    helper = store\n".as_bytes()).await?;
            writer.flush().await?;
        }
        {
            let file = File::create("/home/cratery/.git-credentials").await?;
            let mut writer = BufWriter::new(file);
            let index = self.web_public_uri.find('/').unwrap() + 2;
            writer
                .write_all(
                    format!(
                        "{}{}:{}@{}\n",
                        &self.web_public_uri[..index],
                        self.self_service_login,
                        self.self_service_token,
                        &self.web_public_uri[index..]
                    )
                    .as_bytes(),
                )
                .await?;
            for registry in &self.external_registries {
                let index = registry.index.find('/').unwrap() + 2;
                writer
                    .write_all(
                        format!(
                            "{}{}:{}@{}",
                            &registry.index[..index],
                            registry.login,
                            registry.token,
                            &registry.index[index..]
                        )
                        .as_bytes(),
                    )
                    .await?;
            }
            writer.flush().await?;
        }
        {
            let file = File::create("/home/cratery/.cargo/config.toml").await?;
            let mut writer = BufWriter::new(file);
            writer.write_all("[registry]\n".as_bytes()).await?;
            writer
                .write_all("global-credential-providers = [\"cargo:token\"]\n".as_bytes())
                .await?;
            writer.write_all("\n".as_bytes()).await?;
            writer.write_all("[registries]\n".as_bytes()).await?;
            writer
                .write_all(format!("local = {{ index = \"{}\" }}\n", self.web_public_uri).as_bytes())
                .await?;
            for registry in &self.external_registries {
                writer
                    .write_all(format!("{} = {{ index = \"{}\" }}\n", registry.name, registry.index).as_bytes())
                    .await?;
            }
            writer.flush().await?;
        }
        {
            let file = File::create("/home/cratery/.cargo/credentials").await?;
            let mut writer = BufWriter::new(file);
            writer.write_all("[registries.local]\n".as_bytes()).await?;
            writer
                .write_all(
                    format!(
                        "token = \"Basic {}\"\n",
                        STANDARD.encode(format!("{}:{}", self.self_service_login, self.self_service_token))
                    )
                    .as_bytes(),
                )
                .await?;
            for registry in &self.external_registries {
                writer
                    .write_all(format!("[registries.{}]\n", registry.name).as_bytes())
                    .await?;
                writer
                    .write_all(
                        format!(
                            "token = \"Basic {}\"\n",
                            STANDARD.encode(format!("{}:{}", registry.login, registry.token))
                        )
                        .as_bytes(),
                    )
                    .await?;
            }
            writer.flush().await?;
        }
        Ok(())
    }
}

/// The configuration in the index
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IndexConfig {
    /// The location in the file system
    pub location: String,
    /// URI for the origin git remote to sync with
    #[serde(rename = "remoteOrigin")]
    pub remote_origin: Option<String>,
    /// The name of the file for the SSH key for the remote
    #[serde(rename = "remoteSshKeyFileName")]
    pub remote_ssh_key_file_name: Option<String>,
    /// Do automatically push index changes to the remote
    #[serde(rename = "remotePushChanges")]
    pub remote_push_changes: bool,
    /// The user name to use for commits
    #[serde(rename = "userName")]
    pub user_name: String,
    /// The user email to use for commits
    #[serde(rename = "userEmail")]
    pub user_email: String,
    /// The public configuration
    pub public: IndexPublicConfig,
}

/// The configuration in the index
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IndexPublicConfig {
    /// The root URI to download crates
    pub dl: String,
    /// The API root URI
    pub api: String,
    /// Whether authentication is always required
    #[serde(rename = "auth-required")]
    pub auth_required: bool,
}
