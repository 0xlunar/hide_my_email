# Hide My Email
Rust implementation of Apple's Hide my email service

## Requirements
- Apple ID
- iCloud+ Subscription

## Installation
`cargo add hide_my_email`

## Obtaining Apple Cookies
1. Login to [iCloud](https://www.icloud.com/)
2. Open the Dot Menu *(top right, next to your photo)*
3. Click "Hide My Email" under iCloud+ Features
4. Open your browsers developer tools (F12, or right-click on the page and click "Inspect")
5. Navigate to the Network tab on developer tools and ensure it is recording *(enabling Preserve Log may help)*.
6. Refresh the website
7. In the Network tab, filter the request for "validate", should only be 1, if multiple, use the last request (most recent)
8. Scroll/Navigate to Request Headers section/tab
9. Copy the value for the Cookie header

## Example

```rust
use std::env;
use hide_my_email::{Cookie, HideMyEmailManager, ICloudClient};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
   // eg, key1=value1; key2=\"value2\";
   let cookies = env::var("COOKIE").unwrap();
   let cookies = Cookie::from_str(&cookies).unwrap();

   let mut icloud = ICloudClient::new(&cookies);
   let _ = icloud.validate().await?;
   
   let manager = HideMyEmailManager::from(icloud);

   let email = manager.generate().await?;
   let _ = manager.claim(&email, "RustLang", "").await?;

   // OR
   
   let email = manager.generate_and_claim("RustLang", "").await?;
   
   Ok(())
}
```

## Plan for future
Add authentication support via Username/Password + 2FA instead of cookies
