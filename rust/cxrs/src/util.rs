use sha2::{Digest, Sha256};

pub fn sha256_hex(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    let digest = hasher.finalize();
    format!("{:x}", digest)
}

pub trait IfEmpty {
    fn if_empty_else(self, f: impl FnOnce() -> String) -> String;
}

impl IfEmpty for String {
    fn if_empty_else(self, f: impl FnOnce() -> String) -> String {
        if self.is_empty() { f() } else { self }
    }
}

