use std::{
    fmt,
    hash::{Hash, Hasher},
    sync::Arc,
};

pub struct ActorPath(Arc<str>);

impl ActorPath {
    pub fn new(path: &str) -> Self {
        ActorPath(Arc::from(path))
    }
}

impl AsRef<str> for ActorPath {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl PartialEq for ActorPath {
    fn eq(&self, other: &ActorPath) -> bool {
        self.0 == other.0
    }
}

impl PartialEq<str> for ActorPath {
    fn eq(&self, other: &str) -> bool {
        *self.0 == *other
    }
}

impl Eq for ActorPath {}

impl Hash for ActorPath {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl fmt::Display for ActorPath {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for ActorPath {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl Clone for ActorPath {
    fn clone(&self) -> Self {
        ActorPath(self.0.clone())
    }
}

/// An `ActorUri` represents the location of an actor, including the
/// path and actor system host.
///
/// Note: `host` is currently unused but will be utilized when
/// networking and clustering are introduced.
#[derive(Clone)]
pub struct ActorUri {
    pub name: Arc<str>,
    pub path: ActorPath,
    pub host: Arc<str>,
}

impl PartialEq for ActorUri {
    fn eq(&self, other: &ActorUri) -> bool {
        self.path == other.path
    }
}

impl Eq for ActorUri {}

impl Hash for ActorUri {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.path.hash(state);
    }
}

impl fmt::Display for ActorUri {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.path)
    }
}

impl fmt::Debug for ActorUri {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}://{}", self.host, self.path)
    }
}
