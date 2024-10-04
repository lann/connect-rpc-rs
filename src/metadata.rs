use http::{header::AsHeaderName, HeaderMap, HeaderName, HeaderValue};

use crate::{
    common::{base64_decode, base64_encode},
    Error,
};

const BIN_SUFFIX: &str = "-bin";
const TRAILER_PREFIX: &str = "trailer-";

pub trait Metadata {
    fn get_ascii(&self, key: impl AsHeaderName + AsRef<str>) -> Option<&str>;

    fn get_binary(&self, key: impl AsHeaderName + AsRef<str>) -> Option<Vec<u8>>;

    fn get_all_ascii(&self, key: impl AsHeaderName + AsRef<str>) -> impl Iterator<Item = &str>;

    fn get_all_binary(
        &self,
        key: impl AsHeaderName + AsRef<str>,
    ) -> impl Iterator<Item = Vec<u8>> + '_;

    fn iter_ascii(&self) -> impl Iterator<Item = (&str, &str)>;

    fn iter_binary(&self) -> impl Iterator<Item = (&str, Vec<u8>)>;

    fn insert_ascii(
        &mut self,
        key: impl TryInto<HeaderName, Error: Into<Error>>,
        val: impl Into<String>,
    ) -> Result<(), Error>;

    fn insert_binary(
        &mut self,
        key: impl TryInto<HeaderName, Error: Into<Error>>,
        val: impl AsRef<[u8]>,
    ) -> Result<(), Error>;

    fn append_ascii(
        &mut self,
        key: impl TryInto<HeaderName, Error: Into<Error>>,
        val: impl Into<String>,
    ) -> Result<(), Error>;

    fn append_binary(
        &mut self,
        key: impl TryInto<HeaderName, Error: Into<Error>>,
        val: impl AsRef<[u8]>,
    ) -> Result<(), Error>;
}

impl Metadata for HeaderMap {
    fn get_ascii(&self, key: impl AsHeaderName + AsRef<str>) -> Option<&str> {
        if key.as_ref().ends_with(BIN_SUFFIX) {
            return None;
        }
        get_maybe_trailer(self, key)?.to_str().ok()
    }

    fn get_binary(&self, key: impl AsHeaderName + AsRef<str>) -> Option<Vec<u8>> {
        if !key.as_ref().ends_with(BIN_SUFFIX) {
            return None;
        }
        let b64 = get_maybe_trailer(self, key)?;
        base64_decode(b64).ok()
    }

    fn get_all_ascii(&self, key: impl AsHeaderName + AsRef<str>) -> impl Iterator<Item = &str> {
        let override_empty = key.as_ref().ends_with(BIN_SUFFIX);
        get_all_maybe_trailer(self, key, override_empty).filter_map(|val| val.to_str().ok())
    }

    fn get_all_binary(
        &self,
        key: impl AsHeaderName + AsRef<str>,
    ) -> impl Iterator<Item = Vec<u8>> + '_ {
        let override_empty = !key.as_ref().ends_with(BIN_SUFFIX);
        get_all_maybe_trailer(self, key, override_empty).filter_map(|val| base64_decode(val).ok())
    }

    fn iter_ascii(&self) -> impl Iterator<Item = (&str, &str)> {
        self.iter().filter_map(|(key, val)| {
            let key = key.as_str();
            if key.ends_with(BIN_SUFFIX) {
                return None;
            }
            let key = key.strip_prefix(TRAILER_PREFIX).unwrap_or(key);
            Some((key, val.to_str().ok()?))
        })
    }

    fn iter_binary(&self) -> impl Iterator<Item = (&str, Vec<u8>)> {
        self.iter().filter_map(|(key, val)| {
            let key = key.as_str();
            if !key.ends_with(BIN_SUFFIX) {
                return None;
            }
            let key = key.strip_prefix(TRAILER_PREFIX).unwrap_or(key);
            Some((key, base64_decode(val).ok()?))
        })
    }

    fn insert_ascii(
        &mut self,
        key: impl TryInto<HeaderName, Error: Into<Error>>,
        val: impl Into<String>,
    ) -> Result<(), Error> {
        self.insert(ascii_key(key)?, ascii_value(val)?);
        Ok(())
    }

    fn insert_binary(
        &mut self,
        key: impl TryInto<HeaderName, Error: Into<Error>>,
        val: impl AsRef<[u8]>,
    ) -> Result<(), Error> {
        self.insert(binary_key(key)?, binary_value(val));
        Ok(())
    }

    fn append_ascii(
        &mut self,
        key: impl TryInto<HeaderName, Error: Into<Error>>,
        val: impl Into<String>,
    ) -> Result<(), Error> {
        self.append(ascii_key(key)?, ascii_value(val)?);
        Ok(())
    }

    fn append_binary(
        &mut self,
        key: impl TryInto<HeaderName, Error: Into<Error>>,
        val: impl AsRef<[u8]>,
    ) -> Result<(), Error> {
        self.append(binary_key(key)?, binary_value(val));
        Ok(())
    }
}

fn get_maybe_trailer(
    headers: &HeaderMap,
    key: impl AsHeaderName + AsRef<str>,
) -> Option<&HeaderValue> {
    let trailer_key = format!("{TRAILER_PREFIX}{}", key.as_ref());
    headers.get(key).or_else(|| headers.get(trailer_key))
}

fn get_all_maybe_trailer(
    headers: &HeaderMap,
    key: impl AsHeaderName + AsRef<str>,
    override_empty: bool,
) -> impl Iterator<Item = &'_ HeaderValue> + '_ {
    if override_empty {
        Box::new(std::iter::empty()) as Box<dyn Iterator<Item = &HeaderValue>>
    } else {
        let trailer_key = format!("{TRAILER_PREFIX}{}", key.as_ref());
        Box::new(
            headers
                .get_all(key)
                .into_iter()
                .chain(headers.get_all(trailer_key)),
        )
    }
}

fn ascii_key(key: impl TryInto<HeaderName, Error: Into<Error>>) -> Result<HeaderName, Error> {
    let key = key.try_into().map_err(Into::into)?;
    if key.as_str().ends_with(BIN_SUFFIX) {
        return Err(Error::InvalidMetadata(
            "ASCII metadata keys may not end with '-bin'",
        ));
    }
    Ok(key)
}

fn binary_key(key: impl TryInto<HeaderName, Error: Into<Error>>) -> Result<HeaderName, Error> {
    let key = key.try_into().map_err(Into::into)?;
    if !key.as_str().ends_with(BIN_SUFFIX) {
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
    base64_encode(value).try_into().unwrap()
}
