use std::env;

/// Runtime configuration, read once from the environment at start-up.
#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub jwt_secret: String,
    /// Public base URL (https://sshop.up.railway.app) — used for webhook callbacks and links.
    pub public_url: String,
    /// Directory containing the built web app (index.html + assets).
    pub web_dir: String,
    pub cors_origins: Vec<String>,
    pub bootstrap: Option<Bootstrap>,
    pub mpesa: Option<MpesaConfig>,
    pub whatsapp: Option<WhatsAppConfig>,
    pub email: Option<EmailConfig>,
    /// Platform admins (lower-case emails) review access requests and activate new businesses.
    pub platform_admins: Vec<String>,
    /// Who is emailed when a business requests access.
    pub access_request_notify: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct EmailConfig {
    pub api_key: String,
    /// e.g. "S'Shop <noreply@yourdomain.com>". Resend's test sender only delivers to the account owner.
    pub from: String,
}

#[derive(Clone, Debug)]
pub struct Bootstrap {
    pub business_name: String,
    pub slug: String,
    pub admin_name: String,
    pub admin_email: String,
    pub admin_pin: String,
}

#[derive(Clone, Debug)]
pub struct MpesaConfig {
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
}

#[derive(Clone, Debug)]
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

        let mpesa = match (var("MPESA_CONSUMER_KEY"), var("MPESA_CONSUMER_SECRET"), var("MPESA_SHORTCODE"), var("MPESA_PASSKEY")) {
            (Some(consumer_key), Some(consumer_secret), Some(shortcode), Some(passkey)) => {
                let base_url = match var("MPESA_ENV").as_deref() {
                    Some("production") => "https://api.safaricom.co.ke".to_string(),
                    _ => "https://sandbox.safaricom.co.ke".to_string(),
                };
                let transaction_type = var("MPESA_TRANSACTION_TYPE").unwrap_or_else(|| "CustomerPayBillOnline".into());
                Some(MpesaConfig {
                    base_url,
                    consumer_key,
                    consumer_secret,
                    party_b: var("MPESA_PARTY_B").unwrap_or_else(|| shortcode.clone()),
                    shortcode,
                    passkey,
                    transaction_type,
                    callback_token: var("MPESA_CALLBACK_TOKEN")
                        .ok_or_else(|| anyhow::anyhow!("MPESA_CALLBACK_TOKEN must be set when M-Pesa is configured"))?,
                })
            }
            _ => None,
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
        });

        Ok(Self {
            port: var("PORT").and_then(|p| p.parse().ok()).unwrap_or(8080),
            jwt_secret,
            public_url,
            web_dir: var("WEB_DIR").unwrap_or_else(|| "web/dist".into()),
            cors_origins: var("CORS_ORIGINS")
                .map(|v| v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
                .unwrap_or_default(),
            bootstrap,
            mpesa,
            whatsapp,
            email,
            platform_admins,
            access_request_notify,
        })
    }
}
