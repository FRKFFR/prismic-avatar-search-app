use std::{
    fs,
    path::PathBuf,
    thread,
    time::Duration,
};

use anyhow::{anyhow, Result};
use chrono::Utc;
use eframe::egui::{self, Align, Layout, TextEdit};
use flume::{Receiver, Sender};
use log::warn;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const CONFIG_FILE: &str = "config.json";
const API_BASE: &str = "https://api.vrchat.cloud/api/1";
const USER_AGENT: &str = "VRChatAvatarGUI/0.1";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Config {
    auth_cookie: String,
    user_id: String,
    last_login: Option<String>,
    remember_me: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            auth_cookie: String::new(),
            user_id: String::new(),
            last_login: None,
            remember_me: false,
        }
    }
}

fn config_path() -> PathBuf {
    PathBuf::from(CONFIG_FILE)
}

fn load_config() -> Config {
    let path = config_path();
    if !path.exists() {
        let cfg = Config::default();
        let _ = save_config(&cfg);
        return cfg;
    }

    match fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => Config::default(),
    }
}

fn save_config(config: &Config) -> Result<()> {
    let path = config_path();
    let json = serde_json::to_string_pretty(config)?;
    fs::write(path, json)?;
    Ok(())
}

fn persist_login(auth_cookie: &str, user_id: &str, remember_me: bool) {
    let mut config = load_config();
    if remember_me {
        config.auth_cookie = auth_cookie.to_string();
        config.user_id = user_id.to_string();
        config.last_login = Some(Utc::now().to_rfc3339());
        config.remember_me = true;
    } else {
        config.auth_cookie.clear();
        config.user_id.clear();
        config.last_login = None;
        config.remember_me = false;
    }

    if let Err(err) = save_config(&config) {
        warn!("Failed to save config: {}", err);
    }
}

#[derive(Debug, Clone)]
pub enum WorkerResponse {
    Status(String),
    TwoFactorRequired {
        auth_cookie: String,
        two_factor_type: String,
        remember_me: bool,
    },
    LoginSuccess {
        auth_cookie: String,
        user_id: String,
        remember_me: bool,
    },
    Error(String),
}

#[derive(Clone)]
#[derive(Debug)]
struct TwoFactorState {
    auth_cookie: String,
    two_factor_type: String,
    remember_me: bool,
}

pub fn spawn_worker() -> (Sender<WorkerRequest>, Receiver<WorkerResponse>) {
    let (req_tx, req_rx) = flume::unbounded();
    let (resp_tx, resp_rx) = flume::unbounded();

    thread::spawn(move || {
        let client = match Client::builder()
            .cookie_store(true)
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(30))
            .build()
        {
            Ok(client) => client,
            Err(err) => {
                let _ = resp_tx.send(WorkerResponse::Error(format!(
                    "Failed to build HTTP client: {}",
                    err
                )));
                return;
            }
        };

        while let Ok(request) = req_rx.recv() {
            let outcome = match request {
                WorkerRequest::Login {
                    username,
                    password,
                    remember_me,
                } => handle_login(&client, &resp_tx, username, password, remember_me),
                WorkerRequest::VerifyTwoFactor {
                    code,
                    auth_cookie,
                    two_factor_type,
                    remember_me,
                } => handle_two_factor(
                    &client,
                    &resp_tx,
                    code,
                    auth_cookie,
                    two_factor_type,
                    remember_me,
                ),
                WorkerRequest::VerifyCookie { auth_cookie } => {
                    handle_verify_cookie(&client, &resp_tx, auth_cookie)
                }
            };

            if let Err(err) = outcome {
                let _ = resp_tx.send(WorkerResponse::Error(err.to_string()));
            }
        }
    });

    (req_tx, resp_rx)
}

fn handle_login(
    client: &Client,
    tx: &Sender<WorkerResponse>,
    username: String,
    password: String,
    remember_me: bool,
) -> Result<()> {
    tx.send(WorkerResponse::Status("Contacting VRChat...".into()))?;
    let response = client
        .get(format!("{}/auth/user", API_BASE))
        .basic_auth(username, Some(password))
        .send()?;

    if !response.status().is_success() {
        return Err(anyhow!("Login failed: {}", response.status()));
    }

    let auth_cookie = extract_auth_cookie(&response)?;
    let payload: Value = response.json()?;

    if let Some(reqs) = payload
        .get("requiresTwoFactorAuth")
        .and_then(|v| v.as_array())
    {
        if let Some(first) = reqs.first().and_then(|v| v.as_str()) {
            tx.send(WorkerResponse::TwoFactorRequired {
                auth_cookie,
                two_factor_type: first.to_string(),
                remember_me,
            })?;
            return Ok(());
        }
    }

    let user_id = payload
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    tx.send(WorkerResponse::LoginSuccess {
        auth_cookie,
        user_id,
        remember_me,
    })?;
    Ok(())
}

fn handle_two_factor(
    client: &Client,
    tx: &Sender<WorkerResponse>,
    code: String,
    auth_cookie: String,
    two_factor_type: String,
    remember_me: bool,
) -> Result<()> {
    if code.trim().is_empty() {
        return Err(anyhow!("2FA code cannot be empty"));
    }

    tx.send(WorkerResponse::Status("Verifying two-factor code...".into()))?;

    let endpoint = match two_factor_type.as_str() {
        "emailOtp" => format!("{}/auth/twofactorauth/emailotp/verify", API_BASE),
        _ => format!("{}/auth/twofactorauth/totp/verify", API_BASE),
    };

    let verify_resp = client
        .post(endpoint)
        .header("Cookie", cookie_header(&auth_cookie))
        .json(&json!({ "code": code }))
        .send()?;

    if verify_resp.status().is_success() {
        let payload: Value = verify_resp.json()?;
        let verified = payload
            .get("verified")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if verified {
            tx.send(WorkerResponse::Status(
                "2FA verified. Finalizing login...".into(),
            ))?;
            return complete_with_cookie(client, tx, auth_cookie, remember_me);
        }
    }

    Err(anyhow!("Invalid two-factor code"))
}

fn handle_verify_cookie(
    client: &Client,
    tx: &Sender<WorkerResponse>,
    auth_cookie: String,
) -> Result<()> {
    tx.send(WorkerResponse::Status(
        "Validating saved session...".into(),
    ))?;
    complete_with_cookie(client, tx, auth_cookie, true)
}

fn complete_with_cookie(
    client: &Client,
    tx: &Sender<WorkerResponse>,
    auth_cookie: String,
    remember_me: bool,
) -> Result<()> {
    let response = client
        .get(format!("{}/auth/user", API_BASE))
        .header("Cookie", cookie_header(&auth_cookie))
        .send()?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "Failed to verify session: {}",
            response.status()
        ));
    }

    let payload: Value = response.json()?;
    let user_id = payload
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    tx.send(WorkerResponse::LoginSuccess {
        auth_cookie,
        user_id,
        remember_me,
    })?;
    Ok(())
}

fn extract_auth_cookie(response: &reqwest::blocking::Response) -> Result<String> {
    response
        .cookies()
        .find(|cookie| cookie.name() == "auth")
        .map(|cookie| cookie.value().to_string())
        .ok_or_else(|| anyhow!("Auth cookie not found in response"))
}

fn cookie_header(auth_cookie: &str) -> String {
    format!("auth={}", auth_cookie)
}

#[derive(Debug)]
pub enum WorkerRequest {
    Login {
        username: String,
        password: String,
        remember_me: bool,
    },
    VerifyTwoFactor {
        code: String,
        auth_cookie: String,
        two_factor_type: String,
        remember_me: bool,
    },
    VerifyCookie {
        auth_cookie: String,
    },
}

#[derive(Debug)]
pub struct AppState {
    sender: Sender<WorkerRequest>,
    receiver: Receiver<WorkerResponse>,
    username: String,
    password: String,
    remember_me: bool,
    status: String,
    last_error: Option<String>,
    two_factor_code: String,
    two_factor_state: Option<TwoFactorState>,
    last_login: Option<String>,
    logged_in_user: Option<String>,
}

impl AppState {
    pub fn new(sender: Sender<WorkerRequest>, receiver: Receiver<WorkerResponse>) -> Self {
        let config = load_config();
        let mut state = Self {
            sender,
            receiver,
            username: String::new(),
            password: String::new(),
            remember_me: config.remember_me,
            status: "Enter your VRChat credentials.".into(),
            last_error: None,
            two_factor_code: String::new(),
            two_factor_state: None,
            last_login: config.last_login,
            logged_in_user: if config.user_id.is_empty() {
                None
            } else {
                Some(config.user_id)
            },
        };

        if config.remember_me && !config.auth_cookie.is_empty() {
            let _ = state.sender.send(WorkerRequest::VerifyCookie {
                auth_cookie: config.auth_cookie,
            });
            state.status = "Verifying saved login...".into();
        }

        state
    }

    fn submit_login(&mut self) {
        if self.username.trim().is_empty() || self.password.is_empty() {
            self.last_error = Some("Please enter both username and password.".into());
            return;
        }

        let request = WorkerRequest::Login {
            username: self.username.trim().to_string(),
            password: self.password.clone(),
            remember_me: self.remember_me,
        };

        if self.sender.send(request).is_ok() {
            self.status = "Starting login...".into();
            self.last_error = None;
            self.two_factor_code.clear();
        } else {
            self.last_error = Some("Failed to send login request.".into());
        }
    }

    fn submit_two_factor(&mut self) {
        let Some(state) = self.two_factor_state.clone() else {
            self.last_error = Some("No pending two-factor request.".into());
            return;
        };

        if self.two_factor_code.trim().is_empty() {
            self.last_error = Some("Please enter the 2FA code.".into());
            return;
        }

        let request = WorkerRequest::VerifyTwoFactor {
            code: self.two_factor_code.trim().to_string(),
            auth_cookie: state.auth_cookie,
            two_factor_type: state.two_factor_type,
            remember_me: state.remember_me,
        };

        if self.sender.send(request).is_ok() {
            self.status = "Verifying 2FA code...".into();
            self.last_error = None;
        } else {
            self.last_error = Some("Failed to send verification request.".into());
        }
    }

    pub fn is_logged_in(&self) -> bool {
        self.logged_in_user.is_some()
    }
    
    pub fn poll_worker(&mut self, ctx: &egui::Context) {
        while let Ok(message) = self.receiver.try_recv() {
            match message {
                WorkerResponse::Status(text) => {
                    self.status = text;
                }
                WorkerResponse::TwoFactorRequired {
                    auth_cookie,
                    two_factor_type,
                    remember_me,
                } => {
                    self.two_factor_state = Some(TwoFactorState {
                        auth_cookie,
                        two_factor_type: two_factor_type.clone(),
                        remember_me,
                    });
                    self.two_factor_code.clear();
                    self.status = format!(
                        "Two-factor authentication required ({}).",
                        two_factor_type
                    );
                }
                WorkerResponse::LoginSuccess {
                    auth_cookie,
                    user_id,
                    remember_me,
                } => {
                    persist_login(&auth_cookie, &user_id, remember_me);
                    self.logged_in_user = Some(user_id.clone());
                    self.last_login = if remember_me {
                        Some(Utc::now().to_rfc3339())
                    } else {
                        None
                    };
                    self.status = format!("Logged in as {}", user_id);
                    self.two_factor_state = None;
                    self.two_factor_code.clear();
                    self.password.clear();
                    self.last_error = None;
                }
                WorkerResponse::Error(err) => {
                    self.last_error = Some(err.clone());
                    self.status = "An error occurred.".into();
                }
            }
        }

        ctx.request_repaint();
    }
}

impl eframe::App for AppState {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.poll_worker(ctx);

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("VRChat Login (Rust)");
            ui.separator();

            ui.label(format!("Status: {}", self.status));
            if let Some(ref err) = self.last_error {
                ui.colored_label(egui::Color32::RED, err);
            }
            if let Some(ref user) = self.logged_in_user {
                ui.label(format!("Active session: {}", user));
            }
            if let Some(ref last_login) = self.last_login {
                ui.label(format!("Last login: {}", last_login));
            }

            ui.separator();

            if self.two_factor_state.is_some() {
                ui.label("Enter the 2FA code from your authenticator/email:");
                ui.add(TextEdit::singleline(&mut self.two_factor_code).password(true));

                ui.horizontal(|ui| {
                    if ui.button("Verify Code").clicked() {
                        self.submit_two_factor();
                    }
                    if ui.button("Cancel").clicked() {
                        self.two_factor_state = None;
                        self.two_factor_code.clear();
                        self.status = "Two-factor verification canceled.".into();
                    }
                });
            } else {
                ui.label("Username");
                ui.add(TextEdit::singleline(&mut self.username));

                ui.label("Password");
                ui.add(TextEdit::singleline(&mut self.password).password(true));

                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.remember_me, "Remember me");
                    if ui.button("Login").clicked() {
                        self.submit_login();
                    }
                });
            }

            ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                if ui.button("Quit").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });
    }
}

fn main() {
    env_logger::init();
    let (sender, receiver) = spawn_worker();

    let options = eframe::NativeOptions::default();
    if let Err(err) = eframe::run_native(
        "VRChat Login",
        options,
        Box::new(move |_cc| Box::new(AppState::new(sender.clone(), receiver.clone()))),
    ) {
        eprintln!("Failed to launch login GUI: {err}");
    }
}
