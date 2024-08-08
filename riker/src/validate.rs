use regex::Regex;
use thiserror::Error;
use std::fmt;

pub fn validate_name(name: &str) -> Result<(), InvalidName> {
    let rgx = Regex::new(r"^[a-zA-Z0-9_-]+$").unwrap();
    if !rgx.is_match(name) {
        Err(InvalidName { name: name.into() })
    } else {
        Ok(())
    }
}

#[derive(Clone, Error)]
#[error("InvalidName [{name}] Must contain only a-Z, 0-9, _, or -")]
pub struct InvalidName {
    pub name: String,
}

impl fmt::Debug for InvalidName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.to_string())
    }
}

pub fn validate_path(path: &str) -> Result<(), InvalidPath> {
    let rgx = Regex::new(r"^[a-zA-Z0-9/*._-]+$").unwrap();
    if !rgx.is_match(path) {
        Err(InvalidPath { path: path.into() })
    } else {
        Ok(())
    }
}

#[derive(Clone, Error)]
#[error("InvalidPath [{path}] Must contain only a-Z, 0-9, /, _, .., - or *")]
pub struct InvalidPath {
    path: String,
}

impl fmt::Debug for InvalidPath {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.to_string())
    }
}
