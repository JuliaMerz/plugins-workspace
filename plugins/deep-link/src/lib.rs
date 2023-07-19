// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

use serde::de::DeserializeOwned;
use tauri::{
    plugin::{Builder, PluginApi, TauriPlugin},
    AppHandle, Manager, Runtime,
};

mod commands;
mod config;
mod error;

pub use error::{Error, Result};

#[cfg(target_os = "android")]
const PLUGIN_IDENTIFIER: &str = "app.tauri.deep_link";

fn init_deep_link<R: Runtime, C: DeserializeOwned>(
    app: &AppHandle<R>,
    _api: PluginApi<R, C>,
) -> crate::Result<DeepLink<R>> {
    #[cfg(target_os = "android")]
    {
        use tauri::ipc::{Channel, InvokeBody};

        let handle = _api.register_android_plugin(PLUGIN_IDENTIFIER, "DeepLinkPlugin")?;

        let app_handle = app.clone();
        handle.run_mobile_plugin::<()>(
            "setEventHandler",
            imp::EventHandler {
                handler: Channel::new(move |event| {
                    println!("got channel event: {:?}", &event);

                    let url = match event {
                        InvokeBody::Json(payload) => payload
                            .get("url")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_owned()),
                        _ => None,
                    };

                    let payload = vec![url];
                    app_handle.trigger_global(
                        "deep-link://new-url",
                        Some(serde_json::to_string(&payload).unwrap()),
                    );
                    let _ = app_handle.emit_all("deep-link://new-url", payload);
                    Ok(())
                }),
            },
        )?;

        return Ok(DeepLink(handle));
    }

    #[cfg(not(target_os = "android"))]
    Ok(DeepLink {
        app: app.clone(),
        last_link: Default::default(),
    })
}

#[cfg(target_os = "android")]
mod imp {
    use tauri::{plugin::PluginHandle, Runtime};

    use serde::{Deserialize, Serialize};
    use tauri::ipc::Channel;

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct EventHandler {
        pub handler: Channel,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct LastUrl {
        pub url: Option<url::Url>,
    }

    /// Access to the deep-link APIs.
    pub struct DeepLink<R: Runtime>(pub(crate) PluginHandle<R>);

    impl<R: Runtime> DeepLink<R> {
        /// Get the last saved URL that triggered the deep link.
        pub fn get_last_link(&self) -> crate::Result<Option<Vec<url::Url>>> {
            self.0
                .run_mobile_plugin::<LastUrl>("getLastLink", ())
                .map(|v| v.url.map(|url| vec![url]))
                .map_err(Into::into)
        }
    }
}

#[cfg(not(target_os = "android"))]
mod imp {
    use std::sync::Mutex;
    use tauri::{AppHandle, Runtime};

    /// Access to the deep-link APIs.
    pub struct DeepLink<R: Runtime> {
        #[allow(dead_code)]
        pub(crate) app: AppHandle<R>,
        pub(crate) last_link: Mutex<Option<Vec<url::Url>>>,
    }

    impl<R: Runtime> DeepLink<R> {
        /// Get the last saved URL that triggered the deep link.
        pub fn get_last_link(&self) -> crate::Result<Option<Vec<url::Url>>> {
            Ok(self.last_link.lock().unwrap().clone())
        }
    }
}

pub use imp::DeepLink;

/// Extensions to [`tauri::App`], [`tauri::AppHandle`] and [`tauri::Window`] to access the deep-link APIs.
pub trait DeepLinkExt<R: Runtime> {
    fn deep_link(&self) -> &DeepLink<R>;
}

impl<R: Runtime, T: Manager<R>> crate::DeepLinkExt<R> for T {
    fn deep_link(&self) -> &DeepLink<R> {
        self.state::<DeepLink<R>>().inner()
    }
}

/// Initializes the plugin.
pub fn init<R: Runtime>() -> TauriPlugin<R, Option<config::Config>> {
    Builder::new("deep-link")
        .js_init_script(include_str!("api-iife.js").to_string())
        .invoke_handler(tauri::generate_handler![
            commands::execute,
            commands::get_last_link
        ])
        .setup(|app, api| {
            app.manage(init_deep_link(app, api)?);
            Ok(())
        })
        .on_event(|_app, _event| {
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            if let tauri::RunEvent::Opened { urls } = _event {
                let _ = _app.emit_all("deep-link://new-url", urls);
                _app.state::<DeepLink<R>>()
                    .last_link
                    .lock()
                    .unwrap()
                    .replace(urls.clone());
            }
        })
        .build()
}
