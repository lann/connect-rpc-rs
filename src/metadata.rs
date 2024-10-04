use base64::{engine::general_purpose::STANDARD_NO_PAD as BASE64, Engine};
use http::{
    header::{AsHeaderName, IntoHeaderName},
    HeaderMap, HeaderValue,
};

use crate::Error;

const BIN: &str = "-bin";

pub trait Metadata {
    fn get_ascii(&self, key: impl AsHeaderName + AsRef<str>) -> Option<&str>;

    fn get_binary(&self, key: impl AsHeaderName + AsRef<str>) -> Option<Vec<u8>>;

    fn get_all_ascii(&self, key: impl AsHeaderName + AsRef<str>) -> impl Iterator<Item = &str>;

    fn get_all_binary(
        &self,
        key: impl AsHeaderName + AsRef<str>,
    ) -> impl Iterator<Item = Vec<u8>> + '_;

    fn insert_ascii(
        &mut self,
        key: impl IntoHeaderName + AsRef<str>,
        val: impl Into<String>,
    ) -> Result<(), Error>;

    fn insert_binary(
        &mut self,
        key: impl IntoHeaderName + AsRef<str>,
        val: impl AsRef<[u8]>,
    ) -> Result<(), Error>;

    fn append_ascii(
        &mut self,
        key: impl IntoHeaderName + AsRef<str>,
        val: impl Into<String>,
    ) -> Result<(), Error>;

    fn append_binary(
        &mut self,
        key: impl IntoHeaderName + AsRef<str>,
        val: impl AsRef<[u8]>,
    ) -> Result<(), Error>;
}

impl Metadata for HeaderMap {
    fn get_ascii(&self, key: impl AsHeaderName + AsRef<str>) -> Option<&str> {
        if key.as_ref().ends_with(BIN) {
            return None;
        }
        self.get(key)?.to_str().ok()
    }

    fn get_binary(&self, key: impl AsHeaderName + AsRef<str>) -> Option<Vec<u8>> {
        if !key.as_ref().ends_with(BIN) {
            return None;
        }
        let b64 = self.get(key)?;
        BASE64.decode(b64.as_bytes()).ok()
    }

    fn get_all_ascii(&self, key: impl AsHeaderName + AsRef<str>) -> impl Iterator<Item = &str> {
        if key.as_ref().ends_with(BIN) {
            self.get_all("")
        } else {
            self.get_all(key)
        }
        .into_iter()
        .filter_map(|val| val.to_str().ok())
    }

    fn get_all_binary(
        &self,
        key: impl AsHeaderName + AsRef<str>,
    ) -> impl Iterator<Item = Vec<u8>> + '_ {
        if !key.as_ref().ends_with(BIN) {
            self.get_all("")
        } else {
            self.get_all(key)
        }
        .into_iter()
        .filter_map(|val| BASE64.decode(val).ok())
    }

    fn insert_ascii(
        &mut self,
        key: impl IntoHeaderName + AsRef<str>,
        val: impl Into<String>,
    ) -> Result<(), Error> {
        self.insert(validate_ascii_key(key)?, ascii_value(val)?);
        Ok(())
    }

    fn insert_binary(
        &mut self,
        key: impl IntoHeaderName + AsRef<str>,
        val: impl AsRef<[u8]>,
    ) -> Result<(), Error> {
        self.insert(validate_binary_key(key)?, binary_value(val));
        Ok(())
    }

    fn append_ascii(
        &mut self,
        key: impl IntoHeaderName + AsRef<str>,
        val: impl Into<String>,
    ) -> Result<(), Error> {
        self.append(validate_ascii_key(key)?, ascii_value(val)?);
        Ok(())
    }

    fn append_binary(
        &mut self,
        key: impl IntoHeaderName + AsRef<str>,
        val: impl AsRef<[u8]>,
    ) -> Result<(), Error> {
        self.append(validate_binary_key(key)?, binary_value(val));
        Ok(())
    }
}

fn validate_ascii_key<T: AsRef<str>>(key: T) -> Result<T, Error> {
    if key.as_ref().ends_with(BIN) {
        return Err(Error::InvalidMetadata(
            "ASCII metadata keys may not end with '-bin'",
        ));
    }
    Ok(key)
}

fn validate_binary_key<T: AsRef<str>>(key: T) -> Result<T, Error> {
    if !key.as_ref().ends_with(BIN) {
        return Err(Error::InvalidMetadata(
            "binary metadata keys must end with '-bin'",
        ));
    }
    Ok(key)
}

fn ascii_value(value: impl Into<String>) -> Result<HeaderValue, Error> {
    let value = value.into();
    // ASCII-Value → 1*( %x20-%x7E ) ; space & printable ASCII
    if !value.chars().all(|c| c.is_ascii_graphic() || c == ' ') {
        return Err(Error::InvalidMetadata(
            "ASCII metadata values may only contain printable characters and spaces",
        ));
    }
    Ok(value.try_into()?)
}

fn binary_value(value: impl AsRef<[u8]>) -> HeaderValue {
    let b64 = BASE64.encode(value);
    b64.try_into().unwrap()
}
