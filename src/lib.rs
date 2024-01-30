use std::collections::HashMap;
use anyhow::format_err;
use reqwest::header::{HeaderMap, HeaderValue};
use serde::{Serialize, Deserialize};

pub struct ICloudClient {
    client: reqwest::Client,
    services: HashMap<String, Service>,
    cookies: Vec<Cookie>,
}
pub struct HideMyEmailManager {
    icloud: ICloudClient,
    cookie: String,
}
impl ICloudClient {
    pub fn new(cookies: &[Cookie]) -> ICloudClient {
        let mut headers = HeaderMap::new();
        headers.insert("Origin", HeaderValue::from_static("https://www.icloud.com"));
        headers.insert("Referer", HeaderValue::from_static("https://www.icloud.com/"));
        headers.insert("Accept", HeaderValue::from_static("*/*"));

        let cookie = cookies.iter().map(|c| format!("{}={}", c.name, c.value)).collect::<Vec<String>>().join("; ");
        headers.insert("Cookie", HeaderValue::from_str(&cookie).unwrap());

        let client = reqwest::ClientBuilder::new()
            .default_headers(headers)
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/121.0.0.0 Safari/537.36")
            .cookie_store(true)
            .build().unwrap();

        ICloudClient {
            client,
            services: HashMap::new(),
            cookies: cookies.to_vec(),
        }
    }

    fn setup_url() -> &'static str {
        "https://setup.icloud.com/setup/ws/1"
    }

    pub async fn validate(&mut self) -> anyhow::Result<()> {
        let url = format!("{}/validate", ICloudClient::setup_url());
        let resp = self.client.post(url).send().await?;

        let status = resp.status();
        let mut resp_cookies = resp.cookies().map(|c| Cookie {
            name: c.name().to_string(),
            value: c.value().to_string(),
        }).collect::<Vec<Cookie>>();

        let text = resp.text().await?;
        if status.is_client_error() || status.is_server_error() {
            return Err(format_err!("Failed to send request | Status: {} | Response: {}", status, text));
        }

        let body: ICloudWSValidateResponse = serde_json::from_str(&text)?;

        match body.webservices.get("premiummailsettings") {
            Some(t) => {
                match &t.status {
                    Some(s) => {
                        if s != "active" {
                            return Err(format_err!("Hide my email is inactive/disabled"))
                        }
                    }
                    None => return Err(format_err!("Hide my email missing status"))
                }
            },
            None => return Err(format_err!("Missing Hide my email service"))
        }

        self.services = body.webservices;
        let mut filtered_cookies =  self.cookies.iter().cloned().filter(|c| resp_cookies.iter().find(|r_c| c.name == r_c.name).is_none()).collect::<Vec<Cookie>>();
        filtered_cookies.append(&mut resp_cookies);
        self.cookies = filtered_cookies;

        Ok(())
    }
}
impl HideMyEmailManager {
    pub fn from(icloud: ICloudClient) -> HideMyEmailManager {
        let cookie = icloud.cookies.iter().map(|c| format!("{}={}", c.name, c.value)).collect::<Vec<String>>().join("; ");

        HideMyEmailManager {
            icloud,
            cookie,
        }
    }

    fn base_url(&self) -> Option<String> {
        match self.icloud.services.get("premiummailsettings") {
            Some(s) => s.url.clone().into(),
            None => None
        }
    }

    pub async fn generate(&self) -> anyhow::Result<String> {
        let base = match self.base_url() {
            Some(t) => t,
            None => return Err(format_err!("Missing Base URL"))
        };

        let resp = self.icloud.client.post(format!("{}/v1/hme/generate", base)).header("Cookie", &self.cookie).send().await?;

        let status = resp.status();
        let text = resp.text().await?;
        if status.is_server_error() || status.is_client_error() {
            return Err(format_err!("Failed to send request | Status: {} | Response: {}", status, text));
        }

        let body: HMEResponse = match serde_json::from_str(&text) {
            Ok(t) => t,
            Err(e) => {
                return Err(e.into());
            }
        };

        let hme = match body.result.hme {
            HMEResultType::Generate(g) => g,
            _ => unreachable!(),
        };

        Ok(hme)
    }
    pub async fn claim(&self, email: &str, label: &str, note: &str) -> anyhow::Result<HMEReserveResult> {
        let base = match self.base_url() {
            Some(t) => t,
            None => return Err(format_err!("Missing Base URL"))
        };

        let payload = HMEClaimPayload {
            hme: email.into(),
            label: label.into(),
            note: note.into(),
        };

        let resp = self.icloud.client.post(format!("{}/v1/hme/reserve", base)).header("Cookie", &self.cookie).json(&payload).send().await?;

        let status = resp.status();
        let text = resp.text().await?;
        if status.is_server_error() || status.is_client_error() {
            return Err(format_err!("Failed to send request | Status: {} | Response: {}", status, text));
        }

        let body: HMEResponse = serde_json::from_str(&text)?;
        match body.result.hme {
            HMEResultType::Reserve(g) => {
                if g.is_active && g.hme == email {
                    Ok(g)
                } else {
                    Err(format_err!("Hide my email for {} is inactive/invalid, Active: {}, HME: {}", email, g.is_active, g.hme))
                }
            },
            _ => unreachable!(),
        }
    }
    pub async fn list(&self) -> anyhow::Result<HMEListResult> {
        let base = match self.base_url() {
            Some(t) => t,
            None => return Err(format_err!("Missing Base URL"))
        };
        let resp = self.icloud.client.get(format!("{}/v2/hme/list", base)).header("Cookie", &self.cookie).send().await?;

        let status = resp.status();
        let text = resp.text().await?;
        if status.is_server_error() || status.is_client_error() {
            return Err(format_err!("Failed to send request | Status: {} | Response: {}", status, text));
        }

        let body: HMEListResponse = serde_json::from_str(&text)?;
        Ok(body.result)
    }
    pub async fn generate_and_claim(&self, label: &str, note: &str) -> anyhow::Result<String> {
        let hme = self.generate().await?;
        self.claim(&hme, label, note).await?;
        Ok(hme)
    }
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Default, Debug, Clone)]
pub struct Cookie {
    name: String,
    value: String,
}

#[derive(Debug, Eq, PartialEq)]
struct ParseCookieError<'a>(&'a str);

impl Cookie {
    // Only supports name=value;
    fn from_str(s: &str) -> Result<Vec<Self>, ParseCookieError> {
        let mut cookies = Vec::new();
        let splt = s.split("; ");
        for cookie in splt {
            match cookie.split_once("=") {
                Some((k, v)) => {
                    cookies.push(Self {
                        name: k.to_string(),
                        value: v.to_string(),
                    })
                },
                None => return Err(ParseCookieError(cookie))
            }
        }
        Ok(cookies)
    }
}
#[derive(Deserialize, Debug)]
struct Service {
    url: Option<String>,
    status: Option<String>,
}
#[derive(Deserialize, Debug)]
struct ICloudWSValidateResponse {
    webservices: HashMap<String, Service>
}
#[derive(Deserialize, Debug)]
struct HMEResponse {
    success: bool,
    timestamp: u64,
    result: HMEResult,
}
#[derive(Deserialize, Debug)]
struct HMEListResponse {
    success: bool,
    timestamp: u64,
    result: HMEListResult,
}
#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum HMEResultType {
    Generate(String),
    Reserve(HMEReserveResult),
}
#[derive(Deserialize, Debug)]
struct HMEResult {
    hme: HMEResultType,
}
#[derive(Deserialize, Debug)]
struct HMEReserveResult {
    origin: String,
    #[serde(rename = "anonymousId")]
    anonymous_id: String,
    domain: String,
    hme: String,
    label: String,
    note: String,
    #[serde(rename = "createTimestamp")]
    create_timestamp: u64,
    #[serde(rename = "isActive")]
    is_active: bool,
    #[serde(rename = "recipientMailId")]
    recipient_mail_id: String,
}
#[derive(Deserialize, Debug)]
struct HMEListResult {
    #[serde(rename = "forwardToEmails")]
    forward_to_emails: Vec<String>,
    #[serde(rename = "hmeEmails")]
    hme_emails: Vec<HMEListEmails>,
    #[serde(rename = "selectedForwardTo")]
    selected_forward_to: String,
}
#[derive(Deserialize, Debug)]
struct HMEListEmails {
    origin: String,
    #[serde(rename = "anonymousId")]
    anonymous_id: String,
    domain: String,
    #[serde(rename = "forwardToEmail")]
    forward_to_email: String,
    hme: String,
    label: String,
    note: String,
    #[serde(rename = "createTimestamp")]
    create_timestamp: u64,
    #[serde(rename = "isActive")]
    is_active: bool,
    #[serde(rename = "recipientMailId")]
    recipient_mail_id: String,
}
#[derive(Serialize, Debug)]
struct HMEClaimPayload {
    hme: String,
    label: String,
    note: String,
}

#[cfg(test)]
mod tests {
    use std::env;
    use crate::{Cookie, HideMyEmailManager, ICloudClient, ParseCookieError};

    #[tokio::test]
    async fn generate_hme_and_claim() {
        let cookies = env::var("COOKIE").unwrap();
        let cookies = Cookie::from_str(&cookies).unwrap();

        let mut icloud = ICloudClient::new(&cookies);
        icloud.validate().await.unwrap();
        let manager = HideMyEmailManager::from(icloud);

        let res = manager.generate_and_claim("Rust Test", "").await;
        assert_eq!(res.is_ok(), true);
    }

    #[tokio::test]
    async fn fetch_hme_list() {
        let cookies = env::var("COOKIE").unwrap();
        let cookies = Cookie::from_str(&cookies).unwrap();

        let mut icloud = ICloudClient::new(&cookies);
        icloud.validate().await.unwrap();
        let manager = HideMyEmailManager::from(icloud);

        let res = manager.list().await;

        assert_eq!(res.is_ok(), true);
        let res = res.unwrap();
        assert_eq!(res.hme_emails.iter().find(|e| e.label.to_string() == "Rust Test".to_string()).is_some(), true)
    }

    #[test]
    fn cookie_from_str_empty() {
        let cookie = "";
        let result = Cookie::from_str(cookie);
        assert_eq!(result.is_err(), true);
        assert_eq!(result.unwrap_err(), ParseCookieError(""));
    }

    #[test]
    fn cookie_from_str_valid() {
        let cookie = "x-APPLE-WEBAUTH-PCS-Documents=\"abc123==\"; X-APPLE-WEBAUTH-PCS-Photos=\"123+kv2==\"; X-APPLE-WEBAUTH-PCS-Cloudkit=\"1x73==233==\"; =banana";
        let test_output = vec![
            Cookie {
                name: "x-APPLE-WEBAUTH-PCS-Documents".to_string(),
                value: "\"abc123==\"".to_string(),
            },
            Cookie {
                name: "X-APPLE-WEBAUTH-PCS-Photos".to_string(),
                value: "\"123+kv2==\"".to_string(),
            },
            Cookie {
                name: "X-APPLE-WEBAUTH-PCS-Cloudkit".to_string(),
                value: "\"1x73==233==\"".to_string(),
            },
            Cookie {
                name: "".to_string(),
                value: "banana".to_string(),
            }
        ];
        let result: Result<Vec<Cookie>, ParseCookieError> = Cookie::from_str(cookie);
        assert_eq!(result.is_ok(), true);
        let result = result.unwrap();
        assert_eq!(result.len(), 4);
        for (a, b) in test_output.into_iter().zip(result) {
            assert_eq!(a, b);
        }
    }

    #[test]
    fn cookie_from_str_error() {
        let cookie = "task=4343; session17373; client=abs31";
        let result = Cookie::from_str(cookie);
        assert_eq!(result.is_err(), true);
        assert_eq!(result.unwrap_err(), ParseCookieError("session17373"));
    }
}