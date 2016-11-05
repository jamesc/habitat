// Copyright (c) 2016 Chef Software Inc. and/or applicable contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

pub mod config;

use std::fmt;
use std::path::Path;
use std::sync::mpsc::{sync_channel, SyncSender, Receiver, TryRecvError};
use std::thread;
use std::time::Duration;

use time::{SteadyTime, Duration as TimeDuration};
use hcore::package::PackageIdent;
use hcore::service::ServiceGroup;
use hcore::crypto::default_cache_key_path;
use hcore::fs::{CACHE_ARTIFACT_PATH, FS_ROOT_PATH};

use {PRODUCT, VERSION};
use depot_client::Client;
use common::ui::UI;
use supervisor::{Supervisor, RuntimeConfig};
use topology::Topology;
use package::Package;
use manager::signals;
use manager::census::CensusList;
use manager::service::config::ServiceConfig;
use util;
use error::Result;
use config::{gconfig, UpdateStrategy};

static LOGKEY: &'static str = "SR";
const UPDATE_STRATEGY_FREQUENCY_MS: i64 = 60000;

#[derive(Debug)]
pub struct Service {
    service_group: ServiceGroup,
    supervisor: Supervisor,
    package: Package,
    pub needs_restart: bool,
    topology: Topology, //    config: ServiceConfig,
    update_strategy: UpdateStrategy,
    update_thread_rx: Option<Receiver<Package>>,
}

impl Service {
    pub fn new(service_group: ServiceGroup,
               package: Package,
               topology: Topology,
               update_strategy: UpdateStrategy)
               -> Result<Service> {
        let (svc_user, svc_group) = try!(util::users::get_user_and_group(&package.pkg_install));
        outputln!("Service {} process will run as user={}, group={}",
                  &package.ident(),
                  &svc_user,
                  &svc_group);
        let runtime_config = RuntimeConfig::new(svc_user, svc_group);
        let supervisor = Supervisor::new(package.ident().clone(), runtime_config);
        let update_thread_rx = if update_strategy != UpdateStrategy::None {
            Some(run_update_strategy(package.ident().clone()))
        } else {
            None
        };
        Ok(Service {
            service_group: service_group,
            supervisor: supervisor,
            package: package,
            topology: topology,
            needs_restart: false,
            update_strategy: update_strategy,
            update_thread_rx: update_thread_rx,
        })
    }

    pub fn start(&mut self) -> Result<()> {
        self.supervisor.start()
    }

    pub fn restart(&mut self) -> Result<()> {
        self.needs_restart = false;
        self.supervisor.restart()
    }

    pub fn down(&mut self) -> Result<()> {
        self.supervisor.down()
    }

    pub fn send_signal(&self, signal: u32) -> Result<()> {
        if self.supervisor.pid.is_some() {
            signals::send_signal(self.supervisor.pid.unwrap(), signal)
        } else {
            debug!("No process to send the signal to");
            Ok(())
        }
    }

    pub fn reconfigure(&mut self, census_list: &CensusList) {
        let sg = format!("{}", self.service_group);
        let service_config = ServiceConfig::new(&sg, &self.package, census_list, gconfig().bind());
    }

    pub fn check_for_updated_package(&mut self) {
        if self.update_thread_rx.is_some() {
            match self.update_thread_rx.as_mut().unwrap().try_recv() {
                Ok(package) => {
                    outputln!("Updated {} to {}", self.package, package);
                    self.package = package;
                    self.needs_restart = true;
                }
                Err(TryRecvError::Disconnected) => {
                    outputln!("Software update thread has died; restarting");
                    let receiver = run_update_strategy(self.package.ident().clone());
                    self.update_thread_rx = Some(receiver);
                }
                Err(TryRecvError::Empty) => {}
            }
        }
    }
}

impl fmt::Display for Service {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.package)
    }
}

pub fn run_update_strategy(package_ident: PackageIdent) -> Receiver<Package> {
    let (tx, rx) = sync_channel(0);
    let _ = thread::Builder::new()
        .name(format!("update-{}", package_ident))
        .spawn(move || update_strategy(package_ident, tx));
    rx
}

pub fn update_strategy(package_ident: PackageIdent, tx_to_service: SyncSender<Package>) {
    'check: loop {
        let next_check = SteadyTime::now() +
                         TimeDuration::milliseconds(UPDATE_STRATEGY_FREQUENCY_MS);
        let depot_client = match Client::new(gconfig().url(), PRODUCT, VERSION, None) {
            Ok(client) => client,
            Err(e) => {
                debug!("Failed to create HTTP client: {:?}", e);
                let time_to_wait = next_check - SteadyTime::now();
                thread::sleep(Duration::from_millis(time_to_wait.num_milliseconds() as u64));
                continue 'check;
            }
        };
        match depot_client.show_package(package_ident.clone()) {
            Ok(remote) => {
                let latest_ident: PackageIdent = remote.get_ident().clone().into();
                if latest_ident > package_ident {
                    let mut ui = UI::default();
                    match depot_client.fetch_package(latest_ident.clone(),
                                                     &Path::new(FS_ROOT_PATH)
                                                         .join(CACHE_ARTIFACT_PATH),
                                                     ui.progress()) {
                        Ok(archive) => {
                            debug!("Updater downloaded new package to {:?}", archive);
                            // JW TODO: actually handle verify and unpack results
                            archive.verify(&default_cache_key_path(None)).unwrap();
                            archive.unpack(None).unwrap();
                            let latest_package = Package::load(&latest_ident, None).unwrap();
                            tx_to_service.send(latest_package).unwrap_or_else(|e| {
                                error!("Main thread has gone away; this is a disaster, mate.")
                            });
                        }
                        Err(e) => {
                            debug!("Failed to download package: {:?}", e);
                        }
                    }
                } else {
                    debug!("Package found is not newer than ours");
                }
            }
            Err(e) => {
                debug!("Updater failed to get latest package: {:?}", e);
            }
        }
        let time_to_wait = next_check - SteadyTime::now();
        thread::sleep(Duration::from_millis(time_to_wait.num_milliseconds() as u64));
    }
}
