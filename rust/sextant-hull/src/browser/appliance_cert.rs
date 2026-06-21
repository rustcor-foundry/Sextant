//! `BrowserApp` appliance-cert trust — refresh and forget the selected
//! local-appliance certificate trust entry (feature-paired stubs).

use super::*;

impl BrowserApp {
    #[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
    pub fn refresh_appliance_cert_trust(&mut self) {
        match ApplianceCertTrustStore::load(&self.profile_data_dir) {
            Ok(store) => {
                self.appliance_cert_entries = store.entries;
                if let Some(selected) = self.selected_appliance_cert {
                    if selected >= self.appliance_cert_entries.len() {
                        self.selected_appliance_cert =
                            self.appliance_cert_entries.len().checked_sub(1);
                    }
                }
                self.appliance_cert_status = format!(
                    "Loaded {} local appliance certificate trust entr{}.",
                    self.appliance_cert_entries.len(),
                    if self.appliance_cert_entries.len() == 1 {
                        "y"
                    } else {
                        "ies"
                    }
                );
                self.last_status = self.appliance_cert_status.clone();
                self.last_ok = true;
            }
            Err(error) => {
                self.appliance_cert_status = format!("Failed to load appliance trust: {error}");
                self.last_status = self.appliance_cert_status.clone();
                self.last_ok = false;
                self.validation.error_seen = true;
            }
        }
    }

    #[cfg(not(any(feature = "xilem-shell", feature = "servo-backend")))]
    pub fn refresh_appliance_cert_trust(&mut self) {
        self.appliance_cert_status =
            "Local appliance trust storage is unavailable in this build.".to_string();
        self.last_status = self.appliance_cert_status.clone();
        self.last_ok = false;
    }

    #[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
    pub fn forget_selected_appliance_cert(&mut self) {
        let Some(selected) = self.selected_appliance_cert else {
            self.last_status = "Select a trusted appliance before forgetting it.".to_string();
            self.last_ok = false;
            return;
        };
        let Some(entry) = self.appliance_cert_entries.get(selected).cloned() else {
            self.selected_appliance_cert = None;
            self.last_status = "Selected appliance trust entry is no longer available.".to_string();
            self.last_ok = false;
            return;
        };
        let target = match Url::parse(&entry.origin) {
            Ok(target) => target,
            Err(error) => {
                self.last_status = format!("Trusted appliance origin is invalid: {error}");
                self.last_ok = false;
                self.validation.error_seen = true;
                return;
            }
        };
        match ApplianceCertTrustStore::load(&self.profile_data_dir).and_then(|mut store| {
            let changed = store.forget(&target)?;
            store.save(&self.profile_data_dir)?;
            Ok(changed)
        }) {
            Ok(changed) => {
                self.refresh_appliance_cert_trust();
                self.last_status = if changed {
                    format!(
                        "Forgot local appliance certificate trust for {}.",
                        entry.origin
                    )
                } else {
                    format!("No stored certificate trust found for {}.", entry.origin)
                };
                self.appliance_cert_status = self.last_status.clone();
                self.last_ok = true;
            }
            Err(error) => {
                self.last_status = format!("Failed to forget appliance trust: {error}");
                self.appliance_cert_status = self.last_status.clone();
                self.last_ok = false;
                self.validation.error_seen = true;
            }
        }
    }

    #[cfg(not(any(feature = "xilem-shell", feature = "servo-backend")))]
    pub fn forget_selected_appliance_cert(&mut self) {
        self.appliance_cert_status =
            "Local appliance trust storage is unavailable in this build.".to_string();
        self.last_status = self.appliance_cert_status.clone();
        self.last_ok = false;
    }
}
