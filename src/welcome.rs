use base64::{engine::general_purpose::STANDARD, Engine as _};
use colored::*;
use crossterm::{
  event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
  terminal::{disable_raw_mode, enable_raw_mode},
};
use reqwest::{header::LOCATION, redirect::Policy, Client, Error, StatusCode};
use std::{
  cmp::Ordering,
  env,
  fmt::Display,
  io::{self, IsTerminal, Write},
  time::Duration,
};

use semver::Version;
use serde::Deserialize;
use serde_json::Value;

/// Reusable function that just prints success messages to the console
fn print_info(text: &str, is_secondary: bool) {
  if is_secondary {
    println!("{}", text.green().italic().dimmed());
  } else {
    println!("{}", text.green());
  };
}

/// Prints the AdGuardian ASCII art to console
fn print_ascii_art() {
  let art = r"
 █████╗ ██████╗  ██████╗ ██╗   ██╗ █████╗ ██████╗ ██████╗ ██╗ █████╗ ███╗   ██╗
██╔══██╗██╔══██╗██╔════╝ ██║   ██║██╔══██╗██╔══██╗██╔══██╗██║██╔══██╗████╗  ██║
███████║██║  ██║██║  ███╗██║   ██║███████║██████╔╝██║  ██║██║███████║██╔██╗ ██║
██╔══██║██║  ██║██║   ██║██║   ██║██╔══██║██╔══██╗██║  ██║██║██╔══██║██║╚██╗██║
██║  ██║██████╔╝╚██████╔╝╚██████╔╝██║  ██║██║  ██║██████╔╝██║██║  ██║██║ ╚████║
╚═╝  ╚═╝╚═════╝  ╚═════╝  ╚═════╝ ╚═╝  ╚═╝╚═╝  ╚═╝╚═════╝ ╚═╝╚═╝  ╚═╝╚═╝  ╚═══╝
";
  print_info(art, false);
  print_info("\nWelcome to AdGuardian Terminal Edition!", false);
  print_info(
    "Terminal-based, real-time traffic monitoring and statistics for your AdGuard Home instance",
    true,
  );
  print_info(
    "For documentation and support, please visit: https://github.com/lissy93/adguardian-term",
    true,
  );
}

/// Print error message, along with (optional) stack trace, then exit
fn print_error(message: &str, sub_message: &str, error: Option<&Error>) -> ! {
  eprintln!(
    "{}{}{}",
    message.red(),
    match error {
      Some(err) => format!("\n{}", err).red().dimmed(),
      None => "".red().dimmed(),
    },
    format!("\n{}", sub_message).yellow(),
  );

  std::process::exit(1);
}

/// Given a key, get the value from the environmental variables, and print it to the console
fn get_env(key: &str) -> Result<String, env::VarError> {
  env::var(key).inspect(|v| {
    println!(
      "{}",
      format!(
        "{} is set to {}",
        key.bold(),
        if key.contains("PASSWORD") {
          "******"
        } else {
          v
        }
      )
      .green()
    );
  })
}

/// Given a possibly undefined version number, check if it's present and supported
fn check_version(version: Option<&str>) {
  let min_version = Version::parse("0.107.29").unwrap();

  match version {
    Some(version_str) => {
      match Version::parse(version_str.strip_prefix('v').unwrap_or(version_str)) {
        Ok(adguard_version) if adguard_version < min_version => print_error(
          "AdGuard Home version is too old, and is now unsupported",
          format!(
            "You're running AdGuard {}. Please upgrade to v{} or later.",
            version_str, min_version
          )
          .as_str(),
          None,
        ),
        Ok(_) => {}
        Err(_) => print_error(
          "Unsupported AdGuard Home version",
          "Couldn't parse the version number reported by your AdGuard Home instance.",
          None,
        ),
      }
    }
    None => {
      print_error(
        "Unsupported AdGuard Home version",
        format!(
          concat!(
            "Failed to get the version number of your AdGuard Home instance.\n",
            "This usually means you're running an old, and unsupported version.\n",
            "Please upgrade to v{} or later."
          ),
          min_version
        )
        .as_str(),
        None,
      );
    }
  }
}

/// Run an async operation, retrying on error up to `attempts` times, `delay` apart.
/// Each failure is reported; the last error is returned once attempts are exhausted.
pub async fn with_retries<T, E, F, Fut>(
  attempts: u32,
  delay: Duration,
  label: &str,
  mut operation: F,
) -> Result<T, E>
where
  F: FnMut() -> Fut,
  Fut: std::future::Future<Output = Result<T, E>>,
  E: Display,
{
  let mut attempt = 1;
  loop {
    match operation().await {
      Ok(value) => return Ok(value),
      Err(e) if attempt < attempts => {
        println!(
          "{}",
          format!(
            "{} failed (attempt {}/{}): {}\nRetrying in {}s...",
            label,
            attempt,
            attempts,
            e,
            delay.as_secs()
          )
          .yellow()
        );
        tokio::time::sleep(delay).await;
        attempt += 1;
      }
      Err(e) => return Err(e),
    }
  }
}

/// Check if AdGuard Home runs in GL.iNet mode, which redirects its web UI to the router's login
async fn is_glinet_mode(ip: &str, port: &str, protocol: &str) -> bool {
  let Ok(client) = Client::builder().redirect(Policy::none()).build() else {
    return false;
  };
  let url = format!("{}://{}:{}/", protocol, ip, port);
  let router_url = format!("http://{}", ip);
  client
    .get(&url)
    .timeout(Duration::from_secs(2))
    .send()
    .await
    .is_ok_and(|res| {
      res
        .headers()
        .get(LOCATION)
        .is_some_and(|l| l == &router_url)
    })
}

/// With the users specified AdGuard details, verify the connection.
/// Returns `Err` on a failed connection (so the caller can retry); exits on
/// rejected auth or an unsupported version, which retrying wouldn't fix.
async fn verify_connection(
  client: &Client,
  ip: &str,
  port: &str,
  protocol: &str,
  username: &str,
  password: &str,
) -> Result<(), Box<dyn std::error::Error>> {
  println!(
    "{}",
    "\nVerifying connection to your AdGuard instance...".blue()
  );

  let auth_string = format!("{}:{}", username, password);
  let auth_header_value = format!("Basic {}", STANDARD.encode(&auth_string));
  let mut headers = reqwest::header::HeaderMap::new();
  headers.insert("Authorization", auth_header_value.parse()?);

  let url = format!("{}://{}:{}/control/status", protocol, ip, port);

  match client
    .get(&url)
    .headers(headers)
    .timeout(Duration::from_secs(2))
    .send()
    .await
  {
    Ok(res) if res.status().is_success() => {
      // Get version string (if present), and check if valid - exit if not
      let body: Value = res.json().await?;
      check_version(body["version"].as_str());
      // All good! Print success message :)
      let safe_version = body["version"].as_str().unwrap_or("mystery version");
      println!(
        "{}",
        format!("AdGuard ({}) connection successful!\n", safe_version).green()
      );
      Ok(())
    }
    // Connection failed to authenticate. Print error and exit
    Ok(res) => print_error(
      &format!("Authentication with AdGuard at {}:{} failed", ip, port),
      if res.status() == StatusCode::UNAUTHORIZED && is_glinet_mode(ip, port, protocol).await {
        "AdGuard Home is running in GL.iNet mode (--glinet), which doesn't accept a username and password."
      } else {
        "Check the credentials you passed as environmental variables and try again."
      },
      None,
    ),
    // Connection failed to establish - return so the caller can retry
    Err(e) => Err(e.into()),
  }
}

#[derive(Deserialize)]
struct CratesIoResponse {
  #[serde(rename = "crate")]
  krate: Crate,
}

#[derive(Deserialize)]
struct Crate {
  max_version: String,
}

/// Gets the latest version of the crate from crates.io
async fn get_latest_version(crate_name: &str) -> Result<String, Box<dyn std::error::Error>> {
  let url = format!("https://crates.io/api/v1/crates/{}", crate_name);
  let client = reqwest::Client::new();
  let res = client
    .get(&url)
    .header(
      reqwest::header::USER_AGENT,
      "version_check (adguardian.as93.net)",
    )
    .timeout(Duration::from_secs(2))
    .send()
    .await?;

  if res.status().is_success() {
    let response: CratesIoResponse = res.json().await?;
    Ok(response.krate.max_version)
  } else {
    let status = res.status();
    let body = res.text().await?;
    Err(format!("Request failed with status {}: body: {}", status, body).into())
  }
}

/// Checks for updates to the crate, and prints a message if an update is available
async fn check_for_updates() {
  // Get crate name and version from Cargo.toml
  let crate_name = env!("CARGO_PKG_NAME");
  let crate_version = env!("CARGO_PKG_VERSION");
  println!("{}", "\nChecking for updates...".blue());
  // Parse the current version, and fetch and parse the latest version
  let zero = Version::new(0, 0, 0);
  let current_version = Version::parse(crate_version).unwrap_or_else(|_| zero.clone());
  let latest_version = Version::parse(
    &get_latest_version(crate_name)
      .await
      .unwrap_or_else(|_| "0.0.0".to_string()),
  )
  .unwrap_or_else(|_| zero.clone());

  // Compare the current and latest versions, and print the appropriate message
  if current_version == zero || latest_version == zero {
    println!("{}", "Unable to check for updates".yellow());
    return;
  }
  match current_version.cmp(&latest_version) {
    Ordering::Less => println!(
      "{}",
      format!(
        "A new version of AdGuardian is available.\nUpdate from {} to {} for the best experience",
        current_version.to_string().bold(),
        latest_version.to_string().bold()
      )
      .yellow()
    ),
    Ordering::Equal => println!(
      "{}",
      format!(
        "AdGuardian is up-to-date, running version {}",
        current_version.to_string().bold()
      )
      .green()
    ),
    Ordering::Greater => println!(
      "{}",
      format!(
        "Running a pre-released edition of AdGuardian, version {}",
        current_version.to_string().bold()
      )
      .green()
    ),
  }
}

/// The value to pre-fill for a field's interactive prompt, where a sensible one exists
fn default_for(key: &str) -> Option<&'static str> {
  match key {
    "ADGUARD_IP" => Some("127.0.0.1"),
    "ADGUARD_PORT" => Some("3000"),
    _ => None,
  }
}

/// Read a line from the terminal in raw mode, echoing nothing. Ctrl-C cancels.
fn read_masked() -> io::Result<String> {
  enable_raw_mode()?;
  let result = masked_loop();
  let _ = disable_raw_mode();
  if result.is_ok() {
    println!();
  }
  result
}

fn masked_loop() -> io::Result<String> {
  let mut value = String::new();
  loop {
    if let Event::Key(key) = event::read()? {
      // Windows also reports key releases, which would double each keypress
      if key.kind == KeyEventKind::Release {
        continue;
      }
      let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
      match key.code {
        KeyCode::Enter => return Ok(value),
        KeyCode::Char('c') if ctrl => return Err(io::ErrorKind::Interrupted.into()),
        KeyCode::Char(c) if !ctrl => value.push(c),
        KeyCode::Backspace => {
          value.pop();
        }
        _ => {}
      }
    }
  }
}

/// Print the prompt and read a value, masking secret fields on an interactive terminal
fn read_field(prompt: &ColoredString, secret: bool) -> io::Result<String> {
  print!("{}", prompt);
  io::stdout().flush()?;
  if secret && io::stdin().is_terminal() {
    read_masked()
  } else {
    let mut value = String::new();
    io::stdin().read_line(&mut value)?;
    Ok(value)
  }
}

/// Read a field off the async runtime threads
async fn read_input(prompt: ColoredString, secret: bool) -> io::Result<String> {
  tokio::task::spawn_blocking(move || read_field(&prompt, secret))
    .await
    .expect("input task panicked")
}

/// Print the cancellation notice and exit cleanly
fn exit_interrupted() -> ! {
  println!(
    "{}",
    "\n\nAdGuardian setup interrupted by user, exiting...".yellow()
  );
  std::process::exit(0);
}

/// Prompt for a single field, re-prompting until the input is valid.
/// Masks passwords, applies the field's default on empty input, validates the
/// port is numeric, and exits cleanly if the user interrupts with Ctrl-C.
async fn prompt_for(key: &str) -> Result<String, Box<dyn std::error::Error>> {
  let default = default_for(key);
  let secret = key.contains("PASSWORD");
  loop {
    let hint = default.map(|d| format!(" [{}]", d)).unwrap_or_default();
    let prompt = format!("› Enter a value for {}{}: ", key, hint)
      .blue()
      .bold();

    let input = tokio::select! {
      res = read_input(prompt, secret) => match res {
        Ok(value) => value,
        Err(e) if e.kind() == io::ErrorKind::Interrupted => exit_interrupted(),
        Err(e) => return Err(e.into()),
      },
      _ = tokio::signal::ctrl_c() => exit_interrupted(),
    };

    let value = match input.trim() {
      "" => default.unwrap_or_default(),
      trimmed => trimmed,
    };

    if key == "ADGUARD_PORT" && value.parse::<u16>().is_err() {
      println!("{}", "Port must be a number, and a valid port".yellow());
      continue;
    }
    return Ok(value.to_string());
  }
}

/// Initiate the welcome script
/// This function will:
/// - Print the AdGuardian ASCII art
/// - Check if there's an update available
/// - Check for the required environmental variables
/// - Prompt the user to enter any missing variables
/// - Verify the connection to the AdGuard instance
/// - Verify authentication is successful
/// - Verify the AdGuard Home version is supported
/// - Then either print a success message, or show instructions to fix and exit
pub async fn welcome() -> Result<(), Box<dyn std::error::Error>> {
  print_ascii_art();

  // Check for updates
  check_for_updates().await;

  println!("{}", "\nStarting initialization checks...".blue());

  let client = Client::new();

  // List of available flags, ant their associated env vars
  let flags = [
    ("--adguard-ip", "ADGUARD_IP"),
    ("--adguard-port", "ADGUARD_PORT"),
    ("--adguard-username", "ADGUARD_USERNAME"),
    ("--adguard-password", "ADGUARD_PASSWORD"),
  ];

  let protocol: String = env::var("ADGUARD_PROTOCOL")
    .unwrap_or_else(|_| "http".into())
    .parse()?;
  env::set_var("ADGUARD_PROTOCOL", protocol);

  // Parse command line arguments
  let mut args = std::env::args().peekable();
  while let Some(arg) = args.next() {
    for &(flag, var) in &flags {
      if arg == flag {
        if let Some(value) = args.peek().filter(|v| !v.starts_with("--")) {
          env::set_var(var, value);
          args.next();
        }
      }
    }
  }

  // If any of the env variables or flags are not yet set, prompt the user to enter them
  for &key in &[
    "ADGUARD_IP",
    "ADGUARD_PORT",
    "ADGUARD_USERNAME",
    "ADGUARD_PASSWORD",
  ] {
    if env::var(key).is_err() {
      println!(
        "{}",
        format!("The {} environmental variable is not yet set", key.bold()).yellow()
      );
      env::set_var(key, prompt_for(key).await?);
    }
  }

  // Grab the values of the (now set) environmental variables
  let ip = get_env("ADGUARD_IP")?;
  let port = get_env("ADGUARD_PORT")?;
  let protocol = get_env("ADGUARD_PROTOCOL")?;
  let username = get_env("ADGUARD_USERNAME")?;
  let password = get_env("ADGUARD_PASSWORD")?;

  // Verify we can connect, authenticate, and that the version is supported
  let connected = with_retries(3, Duration::from_secs(5), "AdGuard connection", || {
    verify_connection(&client, &ip, &port, &protocol, &username, &password)
  })
  .await;

  if connected.is_err() {
    print_error(
      &format!(
        "Could not reach AdGuard at {}:{} after 3 attempts",
        ip, port
      ),
      "Please check that AdGuard Home is running and your settings are correct.",
      None,
    );
  }

  Ok(())
}
