//! Utilities for saving and re-using credentials.
//!
//! Users are encouraged to import this module by name and refer to its contents by qualified path, like:
//! ```ignore
//! use spacetimedb_sdk::credentials;
//! fn credential_store() -> credentials::File {
//!     credentials::File::new("my_app")
//! }
//! ```

#[cfg(not(feature = "web"))]
mod native {
    use home::home_dir;
    use spacetimedb_lib::{bsatn, de::Deserialize, ser::Serialize};
    use std::path::PathBuf;
    use thiserror::Error;

    const CREDENTIALS_DIR: &str = ".spacetimedb_client_credentials";

    #[derive(Error, Debug)]
    pub enum CredentialFileError {
        #[error("Failed to determine user home directory as root for credentials storage")]
        DetermineHomeDir,
        #[error("Error creating credential storage directory {path}")]
        CreateDir {
            path: PathBuf,
            #[source]
            source: std::io::Error,
        },
        #[error("Error serializing credentials for storage in file")]
        Serialize {
            #[source]
            source: bsatn::EncodeError,
        },
        #[error("Error writing BSATN-serialized credentials to file {path}")]
        Write {
            path: PathBuf,
            #[source]
            source: std::io::Error,
        },
        #[error("Error reading BSATN-serialized credentials from file {path}")]
        Read {
            path: PathBuf,
            #[source]
            source: std::io::Error,
        },
        #[error("Error deserializing credentials from bytes stored in file {path}")]
        Deserialize {
            path: PathBuf,
            #[source]
            source: bsatn::DecodeError,
        },
    }

    /// A file on disk which stores, or can store, a JWT for authenticating with SpacetimeDB.
    ///
    /// The file does not necessarily exist or store credentials.
    /// If the credentials have been stored previously, they can be accessed with [`File::load`].
    /// New credentials can be saved to disk with [`File::save`].
    pub struct File {
        filename: String,
    }

    #[derive(Serialize, Deserialize)]
    struct Credentials {
        token: String,
    }

    impl File {
        /// Get a handle on a file which stores a SpacetimeDB [`Identity`] and its private access token.
        pub fn new(key: impl Into<String>) -> Self {
            Self { filename: key.into() }
        }

        fn determine_home_dir() -> Result<PathBuf, CredentialFileError> {
            home_dir().ok_or(CredentialFileError::DetermineHomeDir)
        }

        fn ensure_credentials_dir() -> Result<(), CredentialFileError> {
            let mut path = Self::determine_home_dir()?;
            path.push(CREDENTIALS_DIR);
            std::fs::create_dir_all(&path).map_err(|source| CredentialFileError::CreateDir { path, source })
        }

        fn path(&self) -> Result<PathBuf, CredentialFileError> {
            let mut path = Self::determine_home_dir()?;
            path.push(CREDENTIALS_DIR);
            path.push(&self.filename);
            Ok(path)
        }

        /// Store the provided `token` to disk in the file referred to by `self`.
        pub fn save(self, token: impl Into<String>) -> Result<(), CredentialFileError> {
            Self::ensure_credentials_dir()?;
            let creds = bsatn::to_vec(&Credentials { token: token.into() })
                .map_err(|source| CredentialFileError::Serialize { source })?;
            let path = self.path()?;
            std::fs::write(&path, creds).map_err(|source| CredentialFileError::Write { path, source })?;
            Ok(())
        }

        /// Load a saved token from disk in the file referred to by `self`,
        /// if they have previously been stored by [`Self::save`].
        pub fn load(self) -> Result<Option<String>, CredentialFileError> {
            let path = self.path()?;
            let bytes = match std::fs::read(&path) {
                Ok(bytes) => bytes,
                Err(e) if matches!(e.kind(), std::io::ErrorKind::NotFound) => return Ok(None),
                Err(source) => return Err(CredentialFileError::Read { path, source }),
            };
            let creds = bsatn::from_slice::<Credentials>(&bytes)
                .map_err(|source| CredentialFileError::Deserialize { path, source })?;
            Ok(Some(creds.token))
        }
    }
}

#[cfg(not(feature = "web"))]
pub use native::*;

#[cfg(feature = "web")]
mod web {
    use thiserror::Error;

    const STORAGE_PREFIX: &str = "spacetimedb_credentials_";

    #[derive(Error, Debug)]
    pub enum CredentialFileError {
        #[error("localStorage is not available")]
        StorageUnavailable,
        #[error("Error accessing localStorage: {0}")]
        StorageError(String),
    }

    /// A localStorage-backed credential store for WASM builds.
    ///
    /// Drop-in replacement for the native `File` type.
    pub struct File {
        key: String,
    }

    impl File {
        pub fn new(key: impl Into<String>) -> Self {
            Self {
                key: format!("{}{}", STORAGE_PREFIX, key.into()),
            }
        }

        fn get_local_storage() -> Result<web_sys::Storage, CredentialFileError> {
            let window = web_sys::window().ok_or(CredentialFileError::StorageUnavailable)?;
            window
                .local_storage()
                .map_err(|e| CredentialFileError::StorageError(format!("{e:?}")))?
                .ok_or(CredentialFileError::StorageUnavailable)
        }

        pub fn save(self, token: impl Into<String>) -> Result<(), CredentialFileError> {
            let storage = Self::get_local_storage()?;
            storage
                .set_item(&self.key, &token.into())
                .map_err(|e| CredentialFileError::StorageError(format!("{e:?}")))?;
            Ok(())
        }

        pub fn load(self) -> Result<Option<String>, CredentialFileError> {
            let storage = Self::get_local_storage()?;
            match storage.get_item(&self.key) {
                Ok(val) => Ok(val),
                Err(e) => Err(CredentialFileError::StorageError(format!("{e:?}"))),
            }
        }
    }
}

#[cfg(feature = "web")]
pub use web::*;
