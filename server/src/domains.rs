//! Custom domains for business websites (roadmap 56).
//!
//! 1. **Ownership** — the business publishes `_sshop-verify.<domain>` TXT = `sshop-verify=<token>`. Nothing is attached
//!    to S'Shop before that, so nobody can take a domain they do not control.
//! 2. **Attach** — with `RAILWAY_API_TOKEN` set, the domain is added to this Railway service automatically
//!    (`customDomainCreate`) and Railway's routing record (CNAME / ALIAS) and verification TXT are shown to copy;
//!    otherwise the platform owner attaches it on Railway and records the routing target.
//! 3. **Live** — only when `https://<domain>/api/site/whoami` answers from this server (routing and certificate both
//!    work) does the domain become `active` and start serving the business's website.
//!
//! States: unconfigured · dns_required · verifying · points_elsewhere · ssl_pending · active · misconfigured.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::config::RailwayApi;

/// Normalises what a person types ("https://Shop.MyShop.co.ke/about" → "shop.myshop.co.ke"; www is kept — it is its own host) and checks it is a public hostname
/// S'Shop may serve for a business.
pub fn normalise(input: &str, platform_hosts: &[String]) -> Result<String, String> {
    let mut d = input.trim().to_lowercase();
    for p in ["https://", "http://"] {
        if let Some(rest) = d.strip_prefix(p) {
            d = rest.to_string();
        }
    }
    let d = d.split(['/', '?', '#']).next().unwrap_or_default().trim_end_matches('.').to_string();
    let valid = d.len() <= 253
        && d.contains('.')
        && d.split('.').all(|l| !l.is_empty() && l.len() <= 63 && !l.starts_with('-') && !l.ends_with('-') && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
        && d.rsplit('.').next().is_some_and(|tld| tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic()));
    if !valid {
        return Err("Enter a domain like myshop.co.ke".into());
    }
    let reserved = ["railway.app", "up.railway.app", "railway.com", "localhost"];
    if reserved.iter().any(|r| d == *r || d.ends_with(&format!(".{r}"))) || platform_hosts.iter().any(|h| d == *h || d.ends_with(&format!(".{h}"))) {
        return Err("This domain cannot be used for a business website".into());
    }
    Ok(d)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Record {
    /// TXT | CNAME | ALIAS
    pub kind: String,
    /// Host / name to enter at the DNS provider.
    pub name: String,
    pub value: String,
    /// ok | missing | wrong
    pub status: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Check {
    pub status: String,
    pub message: String,
    pub records: Vec<Record>,
    /// Railway's id for the attached domain.
    pub railway_id: Option<String>,
    pub routing_target: Option<String>,
    pub railway_txt_name: Option<String>,
    pub railway_txt_value: Option<String>,
}

/// DNS-over-HTTPS lookup (Cloudflare). Returns the answer data strings.
pub async fn lookup(http: &reqwest::Client, name: &str, kind: &str) -> Result<Vec<String>, String> {
    let res = http
        .get("https://cloudflare-dns.com/dns-query")
        .query(&[("name", name), ("type", kind)])
        .header("accept", "application/dns-json")
        .timeout(std::time::Duration::from_secs(8))
        .send()
        .await
        .map_err(|e| format!("DNS lookup failed: {e}"))?;
    let v: Value = res.json().await.map_err(|e| format!("DNS answer unreadable: {e}"))?;
    Ok(v["Answer"]
        .as_array()
        .map(|a| a.iter().filter_map(|x| x["data"].as_str()).map(|s| s.trim_matches('"').trim_end_matches('.').to_lowercase()).collect())
        .unwrap_or_default())
}

/// Does `https://<domain>` reach this S'Shop server with a valid certificate?
pub async fn reaches_us(domain: &str) -> bool {
    let client = match reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).timeout(std::time::Duration::from_secs(8)).build() {
        Ok(c) => c,
        Err(_) => return false,
    };
    match client.get(format!("https://{domain}/api/site/whoami")).send().await {
        Ok(r) if r.status().is_success() => r.json::<Value>().await.is_ok_and(|v| v["service"] == "sshop" && v["host"] == domain),
        _ => false,
    }
}

async fn railway(http: &reqwest::Client, api: &RailwayApi, query: &str, variables: Value) -> Result<Value, String> {
    let res = http
        .post("https://backboard.railway.com/graphql/v2")
        .bearer_auth(&api.token)
        .json(&json!({ "query": query, "variables": variables }))
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("Railway could not be reached: {e}"))?;
    let v: Value = res.json().await.map_err(|e| format!("Railway answer unreadable: {e}"))?;
    if let Some(err) = v["errors"].as_array().and_then(|e| e.first()) {
        return Err(format!("Railway: {}", err["message"].as_str().unwrap_or("error")));
    }
    Ok(v["data"].clone())
}

/// Adds the domain to this service on Railway; returns (id, routing target, verification TXT token).
pub async fn railway_attach(http: &reqwest::Client, api: &RailwayApi, domain: &str) -> Result<(String, Option<String>, Option<String>), String> {
    let data = railway(
        http,
        api,
        "mutation($input: CustomDomainCreateInput!) { customDomainCreate(input: $input) { id domain status { verificationToken dnsRecords { hostlabel requiredValue status } } } }",
        json!({ "input": { "projectId": api.project_id, "environmentId": api.environment_id, "serviceId": api.service_id, "domain": domain } }),
    )
    .await?;
    let d = &data["customDomainCreate"];
    let id = d["id"].as_str().ok_or("Railway did not return the domain")?.to_string();
    let target = d["status"]["dnsRecords"].as_array().and_then(|r| r.first()).and_then(|r| r["requiredValue"].as_str()).map(|s| s.trim_end_matches('.').to_lowercase());
    let token = d["status"]["verificationToken"].as_str().map(str::to_string);
    Ok((id, target, token))
}

/// Railway's view of the attached domain: (routing target, routing status, certificate status, verification token).
pub async fn railway_status(http: &reqwest::Client, api: &RailwayApi, id: &str) -> Result<(Option<String>, String, String, Option<String>), String> {
    let data = railway(
        http,
        api,
        "query($id: String!, $projectId: String!) { customDomain(id: $id, projectId: $projectId) { status { verificationToken certificateStatus dnsRecords { requiredValue status } } } }",
        json!({ "id": id, "projectId": api.project_id }),
    )
    .await?;
    let st = &data["customDomain"]["status"];
    let rec = st["dnsRecords"].as_array().and_then(|r| r.first());
    Ok((
        rec.and_then(|r| r["requiredValue"].as_str()).map(|s| s.trim_end_matches('.').to_lowercase()),
        rec.and_then(|r| r["status"].as_str()).unwrap_or("PENDING").to_string(),
        st["certificateStatus"].as_str().unwrap_or("PENDING").to_string(),
        st["verificationToken"].as_str().map(str::to_string),
    ))
}

pub async fn railway_detach(http: &reqwest::Client, api: &RailwayApi, id: &str) -> Result<(), String> {
    railway(http, api, "mutation($id: String!) { customDomainDelete(id: $id) }", json!({ "id": id })).await.map(|_| ())
}

/// The routing record a person adds for `domain` (CNAME for a subdomain, ALIAS / flattened CNAME for a root domain).
pub fn routing_record(domain: &str, target: &str, status: &str) -> Record {
    let labels: Vec<&str> = domain.split('.').collect();
    // Registrable part: example.com, or example.co.ke / example.com.au style (two-letter country code after a generic label).
    let generic = ["co", "com", "net", "org", "ac", "go", "or", "ne", "gov", "edu", "sc", "me"];
    let apex = if labels.len() >= 3 && labels[labels.len() - 1].len() == 2 && generic.contains(&labels[labels.len() - 2]) { 3 } else { 2 };
    let root = labels.len() <= apex;
    Record {
        kind: if root { "ALIAS".into() } else { "CNAME".into() },
        name: if root { "@".into() } else { labels[..labels.len() - apex].join(".") },
        value: target.into(),
        status: status.into(),
        note: if root {
            "Root domain: use ALIAS / ANAME or a flattened CNAME (your DNS provider's name for it). Plain A records are not supported.".into()
        } else {
            String::new()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domains_normalised_and_guarded() {
        let p = vec!["s-shop.store".to_string()];
        assert_eq!(normalise(" https://www.MyShop.co.ke/about ", &p).unwrap(), "www.myshop.co.ke");
        assert_eq!(normalise("MyShop.co.ke.", &p).unwrap(), "myshop.co.ke");
        assert_eq!(normalise("shop.example.com", &p).unwrap(), "shop.example.com");
        assert!(normalise("s-shop.store", &p).is_err());
        assert!(normalise("evil.s-shop.store", &p).is_err());
        assert!(normalise("x.up.railway.app", &p).is_err());
        assert!(normalise("not a domain", &p).is_err());
        assert!(normalise("192.168.1.1", &p).is_err());
        assert!(normalise("localhost", &p).is_err());
    }

    #[test]
    fn routing_records() {
        assert_eq!(routing_record("shop.example.com", "abc.up.railway.app", "missing").kind, "CNAME");
        assert_eq!(routing_record("example.com", "abc.up.railway.app", "missing").name, "@");
        assert_eq!(routing_record("myshop.co.ke", "abc.up.railway.app", "missing").kind, "ALIAS");
        assert_eq!(routing_record("www.myshop.co.ke", "abc.up.railway.app", "missing").name, "www");
        assert_eq!(routing_record("a.b.example.com", "abc.up.railway.app", "missing").name, "a.b");
    }
}
