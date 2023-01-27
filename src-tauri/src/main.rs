#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use std::{collections::HashMap, time::Duration};

use anyhow::anyhow;
use log::{error, trace};
use serde::Deserialize;
use serde_json::json;
use tauri::{
    api::http::{Body, ClientBuilder, FormBody, FormPart, HttpRequestBuilder, ResponseType},
    Manager,
};
use tauri_plugin_log::LogTarget;
use tokio::time::sleep;

// Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

pub mod prism_meta;
pub mod storage;

const FLOW_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const CLIENT_ID: &str = "7872a85a-1d8c-415c-a4f4-1a243f40c354";
const SCOPES: &str = "XboxLive.signin offline_access";
const XBL_AUTH_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_AUTH_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const LAUNCHER_AUTH_URL: &str = "https://api.minecraftservices.com/launcher/login";
const ENTITLEMENT_URL: &str = "https://api.minecraftservices.com/entitlements/license?requestId=";

#[tauri::command]
async fn login_msa(app_handle: tauri::AppHandle) -> Option<String> {
    if let Err(e) = login_msa_inner(app_handle).await {
        error!("{:#?}", e);
        Some(format!("{:?}", e))
    } else {
        None
    }
}

async fn login_msa_inner(app_handle: tauri::AppHandle) -> anyhow::Result<()> {
    let client = ClientBuilder::new().build()?;
    let flow_resp = client
        .send(
            HttpRequestBuilder::new("POST", FLOW_URL)?
                .body(Body::Form(FormBody::new(HashMap::from([
                    (
                        "client_id".to_string(),
                        FormPart::Text(CLIENT_ID.to_string()),
                    ),
                    ("scope".to_string(), FormPart::Text(SCOPES.to_string())),
                ]))))
                .response_type(ResponseType::Json),
        )
        .await?
        .read()
        .await?;
    if flow_resp.status != 200 {
        return Err(anyhow!(
            "Server returned error response: {}",
            flow_resp.data.to_string()
        ));
    }
    let flow_resp: DeviceCodeResponse = serde_json::from_value(flow_resp.data)?;
    app_handle.emit_all("auth:msa:login_message", &flow_resp.message)?;
    trace!("Got response {:?}", &flow_resp);
    sleep(Duration::from_secs(flow_resp.interval.into())).await;
    let token = loop {
        let token_resp = client
            .send(
                HttpRequestBuilder::new("POST", TOKEN_URL)?
                    .body(Body::Form(FormBody::new(HashMap::from([
                        (
                            "client_id".to_string(),
                            FormPart::Text(CLIENT_ID.to_string()),
                        ),
                        (
                            "grant_type".to_string(),
                            FormPart::Text(
                                "urn:ietf:params:oauth:grant-type:device_code".to_string(),
                            ),
                        ),
                        (
                            "device_code".to_string(),
                            FormPart::Text(flow_resp.device_code.clone()),
                        ),
                    ]))))
                    .response_type(ResponseType::Json),
            )
            .await?
            .read()
            .await?;
        let token_resp: TokenResponse = serde_json::from_value(token_resp.data)?;
        println!("Got token response {:?}", token_resp);
        match token_resp {
            TokenResponse::Ok {
                access_token,
                refresh_token,
            } => {
                break Token {
                    access: access_token,
                    refresh: refresh_token,
                };
            }
            TokenResponse::Err { error } => match error {
                TokenResponseErrorKind::AuthorizationPending => {
                    sleep(Duration::from_secs(flow_resp.interval.into())).await;
                }
                TokenResponseErrorKind::AuthorizationDeclined => {
                    return Err(anyhow!("Authentication Declined."))
                }
                TokenResponseErrorKind::BadVerificationCode => {
                    return Err(anyhow!("Server claims bad verification code?"))
                }
                TokenResponseErrorKind::ExpiredToken => {
                    return Err(anyhow!("Authentication time excedded"))
                }
            },
        }
    };
    trace!("Got MSA Token: {:?}", token);
    app_handle.emit_all("auth:msa:msa_token", ())?;

    let xbl_resp = client
        .send(
            HttpRequestBuilder::new("POST", XBL_AUTH_URL)?
                .body(Body::Json(json!({
                    "Properties": {
                        "AuthMethod": "RPS",
                        "SiteName": "user.auth.xboxlive.com",
                        "RpsTicket": format!("d={}", token.access)
                    },
                    "RelyingParty": "http://auth.xboxlive.com",
                    "TokenType": "JWT"
                })))
                .response_type(ResponseType::Json),
        )
        .await?
        .read()
        .await?;
    let xbl_resp: XblAuthResponse = serde_json::from_value(xbl_resp.data)?;
    trace!("got XBL response: {:?}", xbl_resp);
    let (token, userhash) = match xbl_resp {
        XblAuthResponse::Ok {
            issue_instant,
            not_after,
            token,
            display_claims,
        } => (token, display_claims.xui[0].uhs.clone()),
        XblAuthResponse::Err { x_err } => {
            return Err(anyhow!(
                "Error {}: {}",
                x_err,
                match x_err {
                    2148916233 => "This Microsoft account does not have an XBox Live profile.",
                    2148916235 => "XBox Live is not available in your country.",
                    2148916236 =>
                        "The account needs adult verification on Xbox page. (South Korea)",
                    2148916237 =>
                        "The account needs adult verification on Xbox page. (South Korea)",
                    2148916238 =>
                        "This Microsoft account is underaged and is not linked to a family.",
                    _ => "Unknown error.",
                }
            ))
        }
    };
    app_handle.emit_all("auth:msa:xbl_token", ())?;

    let xsts_resp = client
        .send(
            HttpRequestBuilder::new("POST", XSTS_AUTH_URL)?
                .body(Body::Json(json!({
                    "Properties": {
                        "SandboxId": "RETAIL",
                        "UserTokens": [token]
                    },
                    "RelyingParty": "rp://api.minecraftservices.com/",
                    "TokenType": "JWT"
                })))
                .response_type(ResponseType::Json),
        )
        .await?
        .read()
        .await?;
    let xsts_resp: XblAuthResponse = serde_json::from_value(xsts_resp.data)?;
    trace!("got XSTS response: {:?}", xsts_resp);
    app_handle.emit_all("auth:msa:xsts_token", ())?;

    let xsts_token = match xsts_resp {
        XblAuthResponse::Ok {
            issue_instant,
            not_after,
            token,
            display_claims,
        } => token,
        XblAuthResponse::Err { x_err } => {
            return Err(anyhow!("Error {} while getting XSTS token", x_err))
        }
    };

    let launcher_resp = client
        .send(
            HttpRequestBuilder::new("POST", LAUNCHER_AUTH_URL)?
                .body(Body::Json(json!({
                    "xtoken": format!("XBL3.0 x={};{}", userhash, xsts_token),
                    "platform": "PC_LAUNCHER"
                })))
                .response_type(ResponseType::Json),
        )
        .await?
        .read()
        .await?;
    app_handle.emit_all("auth:msa:mc_token", ())?;

    let launcher_token: LauncherToken = serde_json::from_value(launcher_resp.data)?;

    trace!("got launcher response: {:?}", launcher_token.access_token);

    let entitlement_resp = client
        .send(
            HttpRequestBuilder::new(
                "GET",
                format!("{}{}", ENTITLEMENT_URL, uuid::Uuid::new_v4()),
            )?
            .header(
                "Authorization",
                format!("Bearer {}", launcher_token.access_token),
            )?
            .response_type(ResponseType::Json),
        )
        .await?
        .read()
        .await?;
    trace!("got entitlement data: {}", entitlement_resp.data);
    Ok(())
}

const ASSETS_URL_BASE: &str = "https://resources.download.minecraft.net/";

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u32,
    interval: u32,
    message: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum TokenResponse {
    Ok {
        access_token: String,
        refresh_token: String,
    },
    Err {
        error: TokenResponseErrorKind,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TokenResponseErrorKind {
    AuthorizationPending,
    AuthorizationDeclined,
    BadVerificationCode,
    ExpiredToken,
}

#[derive(Debug)]
struct Token {
    access: String,
    refresh: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
#[serde(rename_all = "PascalCase")]
enum XblAuthResponse {
    #[serde(rename_all = "PascalCase")]
    Ok {
        issue_instant: String,
        not_after: String,
        token: String,
        display_claims: XblDisplayClaims,
    },
    #[serde(rename_all = "PascalCase")]
    Err { x_err: u32 },
}

#[derive(Debug, Deserialize)]
struct XblDisplayClaims {
    xui: Vec<XblXui>,
}

#[derive(Debug, Deserialize)]
struct XblXui {
    uhs: String,
}

#[derive(Debug, Deserialize)]
struct LauncherToken {
    access_token: String,
}

fn main() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::default()
                .targets([LogTarget::LogDir, LogTarget::Stdout, LogTarget::Webview])
                .build(),
        )
        .invoke_handler(tauri::generate_handler![greet, login_msa])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
