use std::env;

/// Runtime configuration, read once from the environment at start-up.
#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub jwt_secret: String,
    /// Roadmap 91: key for credentials stored at rest (`DATA_ENCRYPTION_KEY`, else derived from `JWT_SECRET`).
    pub data_key: crate::secrets::DataKey,
    /// Roadmap 149: the key being replaced (`DATA_ENCRYPTION_KEY_PREVIOUS`, set only while rotating): stored
    /// credentials it can read are re-encrypted with `data_key` at start-up.
    pub data_key_previous: Option<crate::secrets::DataKey>,
    /// Roadmap 152: the key about to become current (`DATA_ENCRYPTION_KEY_NEXT`, phase A of a rotation): read with it,
    /// never written with it — so an instance still running during the next deploy reads re-encrypted values.
    pub data_key_next: Option<crate::secrets::DataKey>,
    /// Daraja base URLs (`MPESA_SANDBOX_URL` / `MPESA_PRODUCTION_URL` override them, e.g. for the test stand-in).
    pub daraja_sandbox_url: String,
    pub daraja_production_url: String,
    /// Public base URL (https://sshop.up.railway.app) — used for webhook callbacks and links.
    pub public_url: String,
    /// Directory containing the built web app (index.html + assets).
    pub web_dir: String,
    pub cors_origins: Vec<String>,
    pub bootstrap: Option<Bootstrap>,
    pub mpesa: Option<MpesaConfig>,
    /// Why the platform's `MPESA_*` configuration is not in use (shown to the platform owner).
    pub mpesa_problem: Option<String>,
    pub whatsapp: Option<WhatsAppConfig>,
    pub email: Option<EmailConfig>,
    pub paystack: Option<PaystackConfig>,
    /// Platform admins (lower-case emails) review access requests and activate new businesses.
    pub platform_admins: Vec<String>,
    /// Who is emailed when a business requests access.
    pub access_request_notify: Vec<String>,
    /// Railway public API — attaches business websites' custom domains to this service automatically (optional).
    pub railway: Option<RailwayApi>,
}

/// `RAILWAY_API_TOKEN` (account or team token) plus the ids Railway injects into every deployment.
#[derive(Clone)]
pub struct RailwayApi {
    pub token: String,
    pub project_id: String,
    pub environment_id: String,
    pub service_id: String,
}

impl std::fmt::Debug for RailwayApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RailwayApi").field("token", &"<redacted>").field("project_id", &self.project_id).field("service_id", &self.service_id).finish()
    }
}

#[derive(Clone)]
pub struct EmailConfig {
    pub api_key: String,
    /// e.g. "S'Shop <noreply@yourdomain.com>". Resend's test sender only delivers to the account owner.
    pub from: String,
    /// Where replies go (MAIL_REPLY_TO), e.g. the support mailbox.
    pub reply_to: Option<String>,
    /// Resend webhook signing secret (RESEND_WEBHOOK_SECRET, "whsec_…"): delivered / bounced statuses.
    pub webhook_secret: Option<String>,
    /// https://api.resend.com — overridable only for tests against a local stand-in (RESEND_BASE_URL).
    pub base_url: String,
}

impl std::fmt::Debug for EmailConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmailConfig").field("api_key", &"<redacted>").field("from", &self.from).field("webhook", &self.webhook_secret.is_some()).finish()
    }
}

/// Paystack (platform billing). The secret key only ever lives in the environment and on the server.
#[derive(Clone)]
pub struct PaystackConfig {
    pub secret_key: String,
    /// https://api.paystack.co — overridable only for tests against a local stand-in.
    pub base_url: String,
}

impl std::fmt::Debug for PaystackConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaystackConfig").field("secret_key", &"<redacted>").field("base_url", &self.base_url).finish()
    }
}

#[derive(Clone)]
pub struct Bootstrap {
    pub business_name: String,
    pub slug: String,
    pub admin_name: String,
    pub admin_email: String,
    pub admin_pin: String,
}

#[derive(Clone)]
pub struct MpesaConfig {
    /// `sandbox` | `production`
    pub environment: String,
    pub base_url: String,
    pub consumer_key: String,
    pub consumer_secret: String,
    pub shortcode: String,
    pub passkey: String,
    /// "CustomerPayBillOnline" (paybill) or "CustomerBuyGoodsOnline" (till)
    pub transaction_type: String,
    /// PartyB — the till number for Buy Goods, else the shortcode.
    pub party_b: String,
    /// Secret path segment that authenticates Daraja callbacks.
    pub callback_token: String,
    /// The public HTTPS URL given to Daraja: `<public>/api/webhooks/mpesa/<callback_token>`.
    pub callback_url: String,
}

/// The platform's Daraja app from the environment (roadmap 92). Variable names as set on Railway, with the earlier
/// names still accepted:
///
/// | Purpose | Variable | Also read |
/// |---|---|---|
/// | sandbox / production | `MPESA_ENVIRONMENT` | `MPESA_ENV` |
/// | app credentials | `MPESA_CONSUMER_KEY`, `MPESA_CONSUMER_SECRET` | |
/// | Lipa na M-Pesa Online passkey | `MPESA_STK_PASSKEY` | `MPESA_PASSKEY` |
/// | shortcode (sandbox `174379`) | `MPESA_SHORTCODE` | |
/// | callback | `MPESA_CALLBACK_URL` = `https://<domain>/api/webhooks/mpesa/<long secret>` | `MPESA_CALLBACK_TOKEN` (+ `PUBLIC_URL`) |
/// | Paybill / Till | `MPESA_TRANSACTION_TYPE` (`CustomerPayBillOnline` default, `CustomerBuyGoodsOnline`), `MPESA_PARTY_B` | |
///
/// `Ok(None)` when no credentials are set; `Err` explains a configuration that cannot work.
pub fn mpesa_from_env(var: &dyn Fn(&str) -> Option<String>, public_url: &str) -> Result<Option<MpesaConfig>, String> {
    let get = |names: &[&str]| names.iter().find_map(|n| var(n).map(|v| v.trim().to_string()).filter(|v| !v.is_empty()));
    let key = get(&["MPESA_CONSUMER_KEY"]);
    let secret = get(&["MPESA_CONSUMER_SECRET"]);
    let passkey = get(&["MPESA_STK_PASSKEY", "MPESA_PASSKEY"]);
    let shortcode = get(&["MPESA_SHORTCODE"]);
    if key.is_none() && secret.is_none() && passkey.is_none() && shortcode.is_none() {
        return Ok(None);
    }
    let missing: Vec<&str> = [("MPESA_CONSUMER_KEY", &key), ("MPESA_CONSUMER_SECRET", &secret), ("MPESA_STK_PASSKEY", &passkey), ("MPESA_SHORTCODE", &shortcode)]
        .iter()
        .filter(|(_, v)| v.is_none())
        .map(|(n, _)| *n)
        .collect();
    if !missing.is_empty() {
        return Err(format!("Missing {}", missing.join(", ")));
    }
    let (key, secret, passkey, shortcode) = (key.unwrap_or_default(), secret.unwrap_or_default(), passkey.unwrap_or_default(), shortcode.unwrap_or_default());
    if !shortcode.chars().all(|c| c.is_ascii_digit()) || !(5..=10).contains(&shortcode.len()) {
        return Err("MPESA_SHORTCODE must be 5–10 digits (sandbox: 174379)".into());
    }
    let environment = match get(&["MPESA_ENVIRONMENT", "MPESA_ENV"]).map(|v| v.to_lowercase()).as_deref() {
        None | Some("sandbox") => "sandbox".to_string(),
        Some("production") | Some("live") => "production".to_string(),
        Some(other) => return Err(format!("MPESA_ENVIRONMENT must be sandbox or production (not {other})")),
    };
    let transaction_type = get(&["MPESA_TRANSACTION_TYPE"]).unwrap_or_else(|| "CustomerPayBillOnline".into());
    if !["CustomerPayBillOnline", "CustomerBuyGoodsOnline"].contains(&transaction_type.as_str()) {
        return Err("MPESA_TRANSACTION_TYPE must be CustomerPayBillOnline or CustomerBuyGoodsOnline".into());
    }
    let shape = "MPESA_CALLBACK_URL must be https://<your domain>/api/webhooks/mpesa/<a long random secret> (at least 24 letters, digits, - or _)";
    let (callback_url, callback_token) = match (get(&["MPESA_CALLBACK_URL"]), get(&["MPESA_CALLBACK_TOKEN"])) {
        (Some(url), _) => {
            let url = url.trim_end_matches('/').to_string();
            let local = url.starts_with("http://localhost") || url.starts_with("http://127.0.0.1");
            if !url.starts_with("https://") && !local {
                return Err(format!("{shape} — Safaricom only calls HTTPS addresses"));
            }
            let token = url.split_once("/api/webhooks/mpesa/").map(|(_, t)| t.to_string()).unwrap_or_default();
            if token.len() < 24 || !token.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                return Err(shape.into());
            }
            (url, token)
        }
        (None, Some(token)) if token.len() >= 16 => (format!("{}/api/webhooks/mpesa/{token}", public_url.trim_end_matches('/')), token),
        _ => return Err(shape.into()),
    };
    let base_url = if environment == "production" {
        var("MPESA_PRODUCTION_URL").unwrap_or_else(|| "https://api.safaricom.co.ke".into())
    } else {
        var("MPESA_SANDBOX_URL").unwrap_or_else(|| "https://sandbox.safaricom.co.ke".into())
    }
    .trim_end_matches('/')
    .to_string();
    Ok(Some(MpesaConfig {
        environment,
        base_url,
        consumer_key: key,
        consumer_secret: secret,
        party_b: get(&["MPESA_PARTY_B"]).unwrap_or_else(|| shortcode.clone()),
        shortcode,
        passkey,
        transaction_type,
        callback_token,
        callback_url,
    }))
}

#[cfg(test)]
mod mpesa_env_tests {
    use super::*;
    use std::collections::HashMap;

    fn parse(pairs: &[(&str, &str)]) -> Result<Option<MpesaConfig>, String> {
        let m: HashMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        mpesa_from_env(&move |k: &str| m.get(k).cloned(), "https://s-shop.store")
    }
    const OK: [(&str, &str); 6] = [
        ("MPESA_ENVIRONMENT", "sandbox"),
        ("MPESA_CONSUMER_KEY", "key"),
        ("MPESA_CONSUMER_SECRET", "secret"),
        ("MPESA_STK_PASSKEY", "passkey"),
        ("MPESA_SHORTCODE", "174379"),
        ("MPESA_CALLBACK_URL", "https://s-shop.store/api/webhooks/mpesa/Zx9_long-random-secret-1234"),
    ];

    #[test]
    fn railway_names_and_callback_url() {
        let c = parse(&OK).unwrap().unwrap();
        assert_eq!((c.environment.as_str(), c.shortcode.as_str(), c.passkey.as_str()), ("sandbox", "174379", "passkey"));
        assert_eq!(c.callback_token, "Zx9_long-random-secret-1234");
        assert_eq!(c.callback_url, "https://s-shop.store/api/webhooks/mpesa/Zx9_long-random-secret-1234");
        assert!(c.base_url.contains("sandbox"));
        assert_eq!((c.transaction_type.as_str(), c.party_b.as_str()), ("CustomerPayBillOnline", "174379"));
        let shown = format!("{c:?}");
        assert!(!shown.contains("Zx9_long") && !shown.contains("\"passkey\"") && !shown.contains("\"secret\""), "{shown}");
    }

    #[test]
    fn nothing_set_is_off_and_mistakes_are_explained() {
        assert!(parse(&[]).unwrap().is_none());
        assert!(parse(&OK[..5]).unwrap_err().contains("MPESA_CALLBACK_URL"));
        let mut v = OK.to_vec();
        v[5] = ("MPESA_CALLBACK_URL", "http://s-shop.store/api/webhooks/mpesa/Zx9_long-random-secret-1234");
        assert!(parse(&v).unwrap_err().contains("HTTPS"));
        v[5] = ("MPESA_CALLBACK_URL", "https://s-shop.store/api/mpesa/callback");
        assert!(parse(&v).is_err(), "a guessable callback is refused");
        v[5] = OK[5];
        v[4] = ("MPESA_SHORTCODE", "17A379");
        assert!(parse(&v).is_err());
        assert!(parse(&OK[1..3]).unwrap_err().contains("MPESA_STK_PASSKEY"));
        let old = [("MPESA_ENV", "production"), ("MPESA_CONSUMER_KEY", "k"), ("MPESA_CONSUMER_SECRET", "s"), ("MPESA_PASSKEY", "p"), ("MPESA_SHORTCODE", "600000"), ("MPESA_CALLBACK_TOKEN", "abcdefghijklmnop")];
        let c = parse(&old).unwrap().unwrap();
        assert_eq!(c.callback_url, "https://s-shop.store/api/webhooks/mpesa/abcdefghijklmnop");
        assert!(c.base_url.contains("api.safaricom"));
    }
}

#[derive(Clone)]
pub struct WhatsAppConfig {
    pub token: String,
    pub phone_number_id: String,
    pub verify_token: String,
    pub app_secret: Option<String>,
    pub api_version: String,
    /// Approved utility template with a single {{1}} body parameter, used for
    /// business-initiated messages (outside the 24h customer-service window).
    pub notification_template: Option<String>,
    pub template_language: String,
}

fn var(key: &str) -> Option<String> {
    env::var(key).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

impl Config {
    /// Roadmap 152: every key stored credentials may be read with — current, then next, then previous.
    pub fn read_keys(&self) -> Vec<&[u8; 32]> {
        let mut keys = vec![&self.data_key.0];
        keys.extend(self.data_key_next.as_ref().map(|k| &k.0));
        keys.extend(self.data_key_previous.as_ref().map(|k| &k.0));
        keys
    }

    pub fn from_env() -> anyhow::Result<Self> {
        let jwt_secret = var("JWT_SECRET").ok_or_else(|| anyhow::anyhow!("JWT_SECRET must be set"))?;
        if jwt_secret.len() < 32 {
            anyhow::bail!("JWT_SECRET must be at least 32 characters");
        }

        let public_url = var("PUBLIC_URL")
            .or_else(|| var("RAILWAY_PUBLIC_DOMAIN").map(|d| format!("https://{d}")))
            .unwrap_or_else(|| "http://localhost:8080".into())
            .trim_end_matches('/')
            .to_string();

        let bootstrap = match (var("BOOTSTRAP_ADMIN_EMAIL"), var("BOOTSTRAP_ADMIN_PIN")) {
            (Some(admin_email), Some(admin_pin)) => {
                let business_name = var("BOOTSTRAP_BUSINESS_NAME").unwrap_or_else(|| "S'Shop".into());
                Some(Bootstrap {
                    slug: var("BOOTSTRAP_BUSINESS_SLUG").unwrap_or_else(|| crate::util::slugify(&business_name)),
                    business_name,
                    admin_name: var("BOOTSTRAP_ADMIN_NAME").unwrap_or_else(|| "Administrator".into()),
                    admin_email,
                    admin_pin,
                })
            }
            _ => None,
        };

        // The platform's own Daraja app (roadmap 92). A mistake never stops the server: M-Pesa through it is simply off
        // and the reason is shown to the platform owner (Settings → M-Pesa & WhatsApp) and logged.
        let (mpesa, mpesa_problem) = match mpesa_from_env(&var, &public_url) {
            Ok(m) => (m, None),
            Err(problem) => {
                tracing::error!(%problem, "platform M-Pesa configuration ignored");
                (None, Some(problem))
            }
        };

        let whatsapp = match (var("WHATSAPP_TOKEN"), var("WHATSAPP_PHONE_NUMBER_ID")) {
            (Some(token), Some(phone_number_id)) => Some(WhatsAppConfig {
                token,
                phone_number_id,
                verify_token: var("WHATSAPP_VERIFY_TOKEN")
                    .ok_or_else(|| anyhow::anyhow!("WHATSAPP_VERIFY_TOKEN must be set when WhatsApp is configured"))?,
                app_secret: var("WHATSAPP_APP_SECRET"),
                api_version: var("WHATSAPP_API_VERSION").unwrap_or_else(|| "v21.0".into()),
                notification_template: var("WHATSAPP_NOTIFICATION_TEMPLATE"),
                template_language: var("WHATSAPP_TEMPLATE_LANGUAGE").unwrap_or_else(|| "en".into()),
            }),
            _ => None,
        };

        let emails = |v: String| -> Vec<String> { v.split(',').map(|s| s.trim().to_lowercase()).filter(|s| s.contains('@')).collect() };
        let platform_admins = var("PLATFORM_ADMIN_EMAILS").or_else(|| var("BOOTSTRAP_ADMIN_EMAIL")).map(emails).unwrap_or_default();
        let access_request_notify = var("ACCESS_REQUEST_NOTIFY_EMAILS").map(emails).unwrap_or_else(|| platform_admins.clone());
        let email = var("RESEND_API_KEY").map(|api_key| EmailConfig {
            api_key,
            from: var("MAIL_FROM").unwrap_or_else(|| "S'Shop <onboarding@resend.dev>".into()),
            reply_to: var("MAIL_REPLY_TO"),
            webhook_secret: var("RESEND_WEBHOOK_SECRET"),
            base_url: var("RESEND_BASE_URL").unwrap_or_else(|| "https://api.resend.com".into()).trim_end_matches('/').to_string(),
        });

        let paystack = var("PAYSTACK_SECRET_KEY").map(|secret_key| PaystackConfig {
            secret_key,
            base_url: var("PAYSTACK_BASE_URL").unwrap_or_else(|| "https://api.paystack.co".into()).trim_end_matches('/').to_string(),
        });

        let railway = match (var("RAILWAY_API_TOKEN"), var("RAILWAY_PROJECT_ID"), var("RAILWAY_ENVIRONMENT_ID"), var("RAILWAY_SERVICE_ID")) {
            (Some(token), Some(project_id), Some(environment_id), Some(service_id)) => Some(RailwayApi { token, project_id, environment_id, service_id }),
            _ => None,
        };

        Ok(Self {
            port: var("PORT").and_then(|p| p.parse().ok()).unwrap_or(8080),
            data_key: crate::secrets::DataKey(crate::secrets::key_from_env(var("DATA_ENCRYPTION_KEY").as_deref(), &jwt_secret)?),
            data_key_previous: match var("DATA_ENCRYPTION_KEY_PREVIOUS").filter(|k| !k.trim().is_empty()) {
                Some(k) => Some(crate::secrets::DataKey(crate::secrets::key_from_env(Some(&k), "")?)),
                None => None,
            },
            data_key_next: match var("DATA_ENCRYPTION_KEY_NEXT").filter(|k| !k.trim().is_empty()) {
                Some(k) => Some(crate::secrets::DataKey(crate::secrets::key_from_env(Some(&k), "")?)),
                None => None,
            },
            daraja_sandbox_url: var("MPESA_SANDBOX_URL").unwrap_or_else(|| "https://sandbox.safaricom.co.ke".into()).trim_end_matches('/').to_string(),
            daraja_production_url: var("MPESA_PRODUCTION_URL").unwrap_or_else(|| "https://api.safaricom.co.ke".into()).trim_end_matches('/').to_string(),
            jwt_secret,
            mpesa_problem,
            public_url,
            web_dir: var("WEB_DIR").unwrap_or_else(|| "web/dist".into()),
            cors_origins: var("CORS_ORIGINS")
                .map(|v| v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
                .unwrap_or_default(),
            bootstrap,
            mpesa,
            whatsapp,
            email,
            paystack,
            platform_admins,
            access_request_notify,
            railway,
        })
    }
}

// Secrets never reach logs, even if a configuration is debug-printed.
impl std::fmt::Debug for Bootstrap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bootstrap").field("business_name", &self.business_name).field("admin_email", &self.admin_email).field("admin_pin", &"<redacted>").finish()
    }
}
impl std::fmt::Debug for MpesaConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MpesaConfig").field("base_url", &self.base_url).field("shortcode", &self.shortcode).field("transaction_type", &self.transaction_type)
            .field("consumer_secret", &"<redacted>").field("passkey", &"<redacted>").field("callback_token", &"<redacted>").finish()
    }
}
impl std::fmt::Debug for WhatsAppConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WhatsAppConfig").field("phone_number_id", &self.phone_number_id).field("api_version", &self.api_version)
            .field("token", &"<redacted>").field("verify_token", &"<redacted>").field("app_secret", &"<redacted>").finish()
    }
}
