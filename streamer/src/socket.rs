use std::net::{IpAddr, SocketAddr};

#[derive(Clone, Copy, PartialEq)]
pub enum SocketAddrSpace {
    Unspecified,
    Global,
}

impl SocketAddrSpace {
    pub fn new(allow_private_addr: bool) -> Self {
        if allow_private_addr {
            SocketAddrSpace::Unspecified
        } else {
            SocketAddrSpace::Global
        }
    }

    /// Returns true if the IP address is valid.
    pub fn check(&self, addr: &SocketAddr) -> bool {
        if self == &SocketAddrSpace::Unspecified {
            return true;
        }
        // TODO: remove these once IpAddr::is_global is stable.
        match addr.ip() {
            IpAddr::V4(addr) => {
                // TODO: Consider excluding:
                //    addr.is_loopback() || addr.is_link_local()
                // || addr.is_broadcast() || addr.is_documentation()
                // || addr.is_unspecified()
                !addr.is_private()
            }
            IpAddr::V6(_) => {
                // TODO: Consider excluding:
                // addr.is_loopback() || addr.is_unspecified(),
                true
            }
        }
    }
}
