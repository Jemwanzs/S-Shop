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
    /// Host / name to enter at the DNS provider (relative to the domain's zone: `_sshop-verify`, `www`, `@`).
    pub name: String,
    /// The full record name (`_sshop-verify.example.com`) — some providers ask for this instead.
    #[serde(default)]
    pub fqdn: String,
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

/// One DNS record Railway requires for an attached domain (its own names: type, host label, purpose, status).
#[derive(Debug, Clone)]
pub struct RailwayRecord {
    /// CNAME | A | TXT | NS
    pub kind: String,
    pub host: String,
    pub fqdn: String,
    pub value: String,
    /// TRAFFIC_ROUTE | ACME_DNS01_CHALLENGE | …
    pub purpose: String,
    pub propagated: bool,
}

/// Railway's view of an attached domain (roadmap 78, read with the schema of `backboard.railway.com/graphql/v2`).
#[derive(Debug, Clone, Default)]
pub struct RailwayState {
    pub records: Vec<RailwayRecord>,
    /// The traffic-route target (e.g. abc123.up.railway.app).
    pub target: Option<String>,
    /// Every traffic-route record has propagated.
    pub routing_ok: bool,
    /// valid | issuing | validating | failed | pending
    pub certificate: String,
    pub certificate_error: Option<String>,
    pub verified: bool,
    pub verification_host: Option<String>,
    pub verification_token: Option<String>,
}

fn strip_enum(v: &Value, prefix: &str) -> String {
    v.as_str().unwrap_or_default().trim_start_matches(prefix).to_string()
}

/// The domain already attached to this service on Railway, if any (no duplicate attachments).
pub async fn railway_find(http: &reqwest::Client, api: &RailwayApi, domain: &str) -> Result<Option<String>, String> {
    let data = railway(
        http,
        api,
        "query($projectId: String!, $environmentId: String!, $serviceId: String!) { domains(projectId: $projectId, environmentId: $environmentId, serviceId: $serviceId) { customDomains { id domain } } }",
        json!({ "projectId": api.project_id, "environmentId": api.environment_id, "serviceId": api.service_id }),
    )
    .await?;
    Ok(data["domains"]["customDomains"]
        .as_array()
        .and_then(|a| a.iter().find(|d| d["domain"].as_str().is_some_and(|x| x.eq_ignore_ascii_case(domain))))
        .and_then(|d| d["id"].as_str())
        .map(str::to_string))
}

/// Attaches the domain to this service on Railway (or reuses the existing attachment); returns Railway's id.
pub async fn railway_attach(http: &reqwest::Client, api: &RailwayApi, domain: &str) -> Result<String, String> {
    if let Some(id) = railway_find(http, api, domain).await? {
        return Ok(id);
    }
    let data = railway(
        http,
        api,
        "mutation($input: CustomDomainCreateInput!) { customDomainCreate(input: $input) { id } }",
        json!({ "input": { "projectId": api.project_id, "environmentId": api.environment_id, "serviceId": api.service_id, "domain": domain } }),
    )
    .await?;
    data["customDomainCreate"]["id"].as_str().map(str::to_string).ok_or_else(|| "Railway did not return the domain".to_string())
}

pub async fn railway_status(http: &reqwest::Client, api: &RailwayApi, id: &str) -> Result<RailwayState, String> {
    let data = railway(
        http,
        api,
        "query($id: String!, $projectId: String!) { customDomain(id: $id, projectId: $projectId) { status { verified verificationDnsHost verificationToken
             certificateStatus certificateErrorMessage dnsRecords { recordType purpose hostlabel fqdn requiredValue status } } } }",
        json!({ "id": id, "projectId": api.project_id }),
    )
    .await?;
    let st = &data["customDomain"]["status"];
    let records: Vec<RailwayRecord> = st["dnsRecords"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|r| RailwayRecord {
                    kind: strip_enum(&r["recordType"], "DNS_RECORD_TYPE_"),
                    host: r["hostlabel"].as_str().filter(|h| !h.is_empty()).unwrap_or("@").to_string(),
                    fqdn: r["fqdn"].as_str().unwrap_or_default().trim_end_matches('.').to_lowercase(),
                    value: r["requiredValue"].as_str().unwrap_or_default().trim_end_matches('.').to_string(),
                    purpose: strip_enum(&r["purpose"], "DNS_RECORD_PURPOSE_"),
                    propagated: r["status"].as_str() == Some("DNS_RECORD_STATUS_PROPAGATED"),
                })
                .filter(|r| !r.value.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let routes: Vec<&RailwayRecord> = records.iter().filter(|r| r.purpose == "TRAFFIC_ROUTE").collect();
    let certificate = match st["certificateStatus"].as_str().unwrap_or_default() {
        "CERTIFICATE_STATUS_TYPE_VALID" => "valid",
        "CERTIFICATE_STATUS_TYPE_ISSUING" => "issuing",
        "CERTIFICATE_STATUS_TYPE_VALIDATING_OWNERSHIP" => "validating",
        "CERTIFICATE_STATUS_TYPE_ISSUE_FAILED" => "failed",
        _ => "pending",
    }
    .to_string();
    Ok(RailwayState {
        target: routes.first().map(|r| r.value.to_lowercase()),
        routing_ok: !routes.is_empty() && routes.iter().all(|r| r.propagated),
        records,
        certificate,
        certificate_error: st["certificateErrorMessage"].as_str().filter(|m| !m.is_empty()).map(str::to_string),
        verified: st["verified"].as_bool().unwrap_or(false),
        verification_host: st["verificationDnsHost"].as_str().filter(|h| !h.is_empty()).map(|h| h.trim_end_matches('.').to_lowercase()),
        verification_token: st["verificationToken"].as_str().filter(|h| !h.is_empty()).map(str::to_string),
    })
}

pub async fn railway_detach(http: &reqwest::Client, api: &RailwayApi, id: &str) -> Result<(), String> {
    railway(http, api, "mutation($id: String!) { customDomainDelete(id: $id) }", json!({ "id": id })).await.map(|_| ())
}

/// The routing record a person adds for `domain` (CNAME for a subdomain, ALIAS / flattened CNAME for a root domain).
/// The DNS zone a domain lives in (what is bought from the registrar): example.com, or example.co.ke /
/// example.com.au style (two-letter country code after a generic label).
pub fn zone(domain: &str) -> String {
    let labels: Vec<&str> = domain.split('.').collect();
    let generic = ["co", "com", "net", "org", "ac", "go", "or", "ne", "gov", "edu", "sc", "me"];
    let apex = if labels.len() >= 3 && labels[labels.len() - 1].len() == 2 && generic.contains(&labels[labels.len() - 2]) { 3 } else { 2 };
    labels[labels.len().saturating_sub(apex)..].join(".")
}

/// The host to type at the DNS provider for a full record name: relative to the zone (`@` for the zone itself).
pub fn host(fqdn: &str, domain: &str) -> String {
    let z = zone(domain);
    if fqdn == z {
        "@".into()
    } else {
        fqdn.strip_suffix(&format!(".{z}")).unwrap_or(fqdn).to_string()
    }
}

/// Where a record ends up when the full name is typed into a provider that adds the zone itself
/// (`_sshop-verify.example.com.example.com`) — the most common set-up mistake.
pub fn doubled(fqdn: &str, domain: &str) -> String {
    format!("{fqdn}.{}", zone(domain))
}

/// A TXT record to show, named relative to the zone.
pub fn txt_record(fqdn: &str, domain: &str, value: String, status: &str, note: &str) -> Record {
    Record { kind: "TXT".into(), name: host(fqdn, domain), fqdn: fqdn.into(), value, status: status.into(), note: note.into() }
}

pub fn routing_record(domain: &str, target: &str, status: &str) -> Record {
    let name = host(domain, domain);
    let root = name == "@";
    Record {
        kind: if root { "ALIAS".into() } else { "CNAME".into() },
        name,
        fqdn: domain.into(),
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

    #[test]
    fn record_hosts_relative_to_zone() {
        assert_eq!(host("_sshop-verify.s-shop.click", "s-shop.click"), "_sshop-verify");
        assert_eq!(host("_sshop-verify.shop.example.co.ke", "shop.example.co.ke"), "_sshop-verify.shop");
        assert_eq!(host("example.com", "example.com"), "@");
        assert_eq!(doubled("_sshop-verify.s-shop.click", "s-shop.click"), "_sshop-verify.s-shop.click.s-shop.click");
    }
}
