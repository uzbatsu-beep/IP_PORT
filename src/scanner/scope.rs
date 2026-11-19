//! Strict, non-expanding exclusions applied after forward DNS resolution.
//! Excluded hosts never enter the port-probe queue.
use ipnet::IpNet;
use std::net::{IpAddr, Ipv4Addr};

pub const MAX_EXCLUSION_RULES: usize = 4_096;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Rule {
    Network(IpNet),
    Range(IpAddr, IpAddr),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Exclusions {
    rules: Vec<Rule>,
}

impl Exclusions {
    pub fn parse(input: &str) -> Result<Self, String> {
        let mut rules = Vec::new();
        for raw in input.lines() {
            let line = raw.split('#').next().unwrap_or("");
            for item in line
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
            {
                let rule = if let Ok(net) = item.parse::<IpNet>() {
                    Rule::Network(net)
                } else if let Ok(ip) = item.parse::<IpAddr>() {
                    Rule::Network(IpNet::from(ip))
                } else if let Some((left, right)) = item.split_once('-') {
                    let start: IpAddr = left
                        .trim()
                        .parse()
                        .map_err(|_| format!("Invalid exclusion start: '{left}'"))?;
                    let end: IpAddr = match right.trim().parse() {
                        Ok(ip) => ip,
                        Err(_) => {
                            // Convenient IPv4 last-octet form: 192.168.1.200-254.
                            let IpAddr::V4(first) = start else {
                                return Err(format!("Invalid exclusion end: '{right}'"));
                            };
                            let octet: u8 = right
                                .trim()
                                .parse()
                                .map_err(|_| format!("Invalid exclusion end: '{right}'"))?;
                            let mut parts = first.octets();
                            parts[3] = octet;
                            IpAddr::V4(Ipv4Addr::from(parts))
                        }
                    };
                    if start.is_ipv4() != end.is_ipv4() || start > end {
                        return Err(format!("Invalid exclusion range: '{item}'"));
                    }
                    Rule::Range(start, end)
                } else {
                    return Err(format!("Invalid exclusion rule: '{item}'"));
                };
                if rules.len() >= MAX_EXCLUSION_RULES {
                    return Err(format!(
                        "Too many exclusion rules (max {MAX_EXCLUSION_RULES})"
                    ));
                }
                rules.push(rule);
            }
        }
        Ok(Self { rules })
    }

    pub fn contains(&self, ip: &IpAddr) -> bool {
        self.rules.iter().any(|rule| match rule {
            Rule::Network(net) => net.contains(ip),
            Rule::Range(start, end) => start <= ip && ip <= end,
        })
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_ipv4_ipv6_cidr_and_short_ranges_without_expansion() {
        let rules =
            Exclusions::parse("192.168.1.0/24, 10.0.0.200-254\n2001:db8::/64\n::1 # localhost")
                .unwrap();
        for address in ["192.168.1.42", "10.0.0.250", "2001:db8::abcd", "::1"] {
            assert!(rules.contains(&address.parse().unwrap()), "{address}");
        }
        for address in ["10.0.0.199", "192.168.2.1", "2001:db9::1", "127.0.0.1"] {
            assert!(!rules.contains(&address.parse().unwrap()), "{address}");
        }
    }

    #[test]
    fn rejects_bad_rules_and_reversed_or_mixed_ranges() {
        for invalid in [
            "10.0.0.99-2",
            "10.0.0.1-::1",
            "bad-host",
            "fe80::1-2",
            "10.0.0.1-999",
            "192.168.1.999",
            "10.0.0.0/33",
        ] {
            assert!(Exclusions::parse(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn empty_lines_and_comments_mean_no_exclusions() {
        assert!(Exclusions::parse("  # a comment\n\n").unwrap().is_empty());
    }
}
