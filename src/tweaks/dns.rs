//! DNS manager: benchmark public resolvers with real queries and switch to one.
//! Provider list: winutil dns.json (MIT). "Find fastest DNS" idea: Sparkle.

use crate::util::{self, fs as bfs};
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

const DNS_JSON: &str = include_str!("../../third_party/winutil/dns.json");

#[derive(Clone, Debug, serde::Serialize)]
pub struct Provider {
    pub name: String,
    pub primary: String,
    pub secondary: String,
    pub primary6: String,
    pub secondary6: String,
    /// median latency in ms, None = not measured / unreachable
    pub latency: Option<f64>,
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct Raw {
    #[serde(rename = "Primary")]
    primary: String,
    #[serde(rename = "Secondary")]
    secondary: String,
    #[serde(rename = "Primary6")]
    primary6: String,
    #[serde(rename = "Secondary6")]
    secondary6: String,
}

pub fn providers() -> Vec<Provider> {
    let json = lenient(DNS_JSON);
    let map: std::collections::BTreeMap<String, Raw> = serde_json::from_str(&json).unwrap_or_default();
    map.into_iter()
        .filter(|(_, r)| !r.primary.is_empty())
        .map(|(k, r)| Provider {
            name: k.replace('_', " "),
            primary: r.primary,
            secondary: r.secondary,
            primary6: r.primary6,
            secondary6: r.secondary6,
            latency: None,
        })
        .collect()
}

fn lenient(s: &str) -> String {
    // dns.json is plain JSON today; strip a BOM just in case
    s.trim_start_matches('\u{feff}').to_string()
}

fn query_packet(id: u16, name: &str) -> Vec<u8> {
    let mut p = vec![(id >> 8) as u8, id as u8, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0];
    for label in name.split('.') {
        p.push(label.len() as u8);
        p.extend_from_slice(label.as_bytes());
    }
    p.extend_from_slice(&[0, 0, 1, 0, 1]);
    p
}

/// Median round-trip of 3 real A-record queries, in ms.
pub fn measure(ip: &str) -> Option<f64> {
    let addr: SocketAddr = format!("{ip}:53").parse().ok()?;
    let sock = UdpSocket::bind(if addr.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" }).ok()?;
    sock.set_read_timeout(Some(Duration::from_millis(1500))).ok()?;
    let mut times = Vec::new();
    for (i, host) in ["example.com", "github.com", "wikipedia.org"].iter().enumerate() {
        let id = 0x4242 + i as u16;
        let t = Instant::now();
        if sock.send_to(&query_packet(id, host), addr).is_err() {
            continue;
        }
        let mut buf = [0u8; 512];
        if let Ok((n, _)) = sock.recv_from(&mut buf) {
            if n > 2 && u16::from_be_bytes([buf[0], buf[1]]) == id {
                times.push(t.elapsed().as_secs_f64() * 1000.0);
            }
        }
    }
    if times.is_empty() {
        return None;
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    Some(times[times.len() / 2])
}

pub fn benchmark(list: &mut [Provider]) {
    use rayon::prelude::*;
    list.par_iter_mut().for_each(|p| p.latency = measure(&p.primary));
    list.sort_by(|a, b| match (a.latency, b.latency) {
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap(),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        _ => a.name.cmp(&b.name),
    });
}

/// DNS servers currently in use (IPv4).
pub fn current() -> Vec<String> {
    #[cfg(windows)]
    {
        let out = util::ps("Get-NetAdapter | Where-Object Status -eq 'Up' | ForEach-Object { (Get-DnsClientServerAddress -InterfaceIndex $_.ifIndex -AddressFamily IPv4).ServerAddresses } | Select-Object -Unique").unwrap_or_default();
        out.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect()
    }
    #[cfg(not(windows))]
    {
        let text = std::fs::read_to_string("/etc/resolv.conf").unwrap_or_default();
        let mut v: Vec<String> = text.lines().filter_map(|l| l.strip_prefix("nameserver ")).map(|s| s.trim().to_string()).collect();
        if v.iter().all(|s| s.starts_with("127.")) && util::has_cmd("resolvectl") {
            let out = util::run("resolvectl", &["dns"]).unwrap_or_default();
            v = out.split_whitespace().filter(|w| w.parse::<std::net::IpAddr>().is_ok()).map(String::from).collect();
        }
        v
    }
}

/// Switch every active adapter to `p`, or back to automatic (DHCP) when `p` is None.
pub fn set(p: Option<&Provider>) -> Result<String, String> {
    if bfs::dry() {
        return Ok("dry run".into());
    }
    util::log(format!("dns -> {}", p.map(|p| p.name.as_str()).unwrap_or("automatic")));
    #[cfg(windows)]
    {
        let script = match p {
            Some(p) => {
                let mut ips = vec![util::ps_quote(&p.primary), util::ps_quote(&p.secondary)];
                if !p.primary6.is_empty() {
                    ips.push(util::ps_quote(&p.primary6));
                    ips.push(util::ps_quote(&p.secondary6));
                }
                format!("Get-NetAdapter | Where-Object Status -eq 'Up' | ForEach-Object {{ Set-DnsClientServerAddress -InterfaceIndex $_.ifIndex -ServerAddresses @({}) }}; Clear-DnsClientCache; 'ok'", ips.join(","))
            }
            None => "Get-NetAdapter | Where-Object Status -eq 'Up' | ForEach-Object { Set-DnsClientServerAddress -InterfaceIndex $_.ifIndex -ResetServerAddresses }; Clear-DnsClientCache; 'ok'".into(),
        };
        let out = util::ps(&script).map_err(|e| e.to_string())?;
        if out.contains("ok") {
            Ok("DNS changed".into())
        } else {
            Err(out)
        }
    }
    #[cfg(target_os = "macos")]
    {
        let services = util::run("networksetup", &["-listallnetworkservices"]).unwrap_or_default();
        let mut changed = 0;
        for svc in services.lines().skip(1).filter(|l| !l.starts_with('*')) {
            let mut args = vec!["-setdnsservers".to_string(), svc.to_string()];
            match p {
                Some(p) => args.extend([p.primary.clone(), p.secondary.clone()]),
                None => args.push("Empty".into()),
            }
            let a: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            if util::run_status("networksetup", &a).0 == 0 {
                changed += 1;
            }
        }
        let _ = util::run_status("dscacheutil", &["-flushcache"]);
        return if changed > 0 {
            Ok(format!("{changed} network services updated"))
        } else {
            Err("networksetup failed - run Broom with sudo".into())
        };
    }
    #[cfg(target_os = "linux")]
    {
        let links = util::run("resolvectl", &["status", "--no-pager"]).unwrap_or_default();
        let ifaces: Vec<String> =
            links.lines().filter_map(|l| l.strip_prefix("Link ")).filter_map(|l| l.split(['(', ')']).nth(1).map(String::from)).collect();
        if ifaces.is_empty() {
            return Err("systemd-resolved not found; set DNS in your network settings".into());
        }
        for i in &ifaces {
            let r = match p {
                Some(p) => util::run_status("resolvectl", &["dns", i, &p.primary, &p.secondary]),
                None => util::run_status("resolvectl", &["revert", i]),
            };
            if r.0 != 0 {
                return Err(format!("resolvectl failed (needs sudo): {}", r.1.trim()));
            }
        }
        return Ok("DNS changed until next reboot/reconnect".into());
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn providers_parse() {
        let p = super::providers();
        assert!(p.iter().any(|x| x.primary == "1.1.1.1"));
    }
    #[test]
    fn packet() {
        let p = super::query_packet(1, "a.bc");
        assert_eq!(&p[12..], &[1, b'a', 2, b'b', b'c', 0, 0, 1, 0, 1]);
    }
}
