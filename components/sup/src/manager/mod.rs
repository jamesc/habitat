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

pub mod census;
pub mod service;
pub mod signals;

use hcore::crypto::{default_cache_key_path, SymKey};
use hcore::service::ServiceGroup;
use butterfly::server::Server;
use butterfly::member::Member;
use butterfly::trace::Trace;
use butterfly::rumor::service::Service as ServiceRumor;
use butterfly::server::timing::Timing;

use error::{Error, Result};
use config::{gconfig, UpdateStrategy};
use package::Package;
use util;
use manager::service::Service;
use manager::census::{CensusUpdate, CensusList, CensusEntry};
use topology::Topology;
use manager::signals::SignalEvent;

// TODO: Add update strategies

static LOGKEY: &'static str = "MR";

#[derive(Debug)]
pub struct Manager {
    butterfly: Server,
    services: Vec<Service>,
    census_list: CensusList,
}

impl Manager {
    pub fn new() -> Result<Manager> {
        let mut member = Member::new();
        member.set_persistent(gconfig().gossip_permanent());
        let ring_key = match gconfig().ring() {
            &Some(ref ring_with_revision) => {
                Some(try!(SymKey::get_latest_pair_for(&ring_with_revision,
                                                      &default_cache_key_path(None))))
            }
            &None => None,
        };
        let server = try!(Server::new(gconfig().swim_listen(),
                                      gconfig().gossip_listen(),
                                      member,
                                      Trace::default(),
                                      ring_key,
                                      None));
        Ok(Manager {
            butterfly: server,
            services: Vec::new(),
            census_list: CensusList::new(),
        })
    }

    pub fn add_service(&mut self,
                       package: Package,
                       topology: Topology,
                       update_strategy: UpdateStrategy)
                       -> Result<()> {
        let service_group = ServiceGroup::new(package.name.clone(),
                                              gconfig().group().to_string(),
                                              gconfig().organization().clone());
        let hostname = try!(util::sys::hostname());
        let ip = try!(util::sys::ip());
        // NOTE: We should do this much earlier, to confirm that the ports we expose are not
        //       bullshit.
        let mut exposes = Vec::new();
        for port in package.exposes().into_iter() {
            let port_num = try!(port.parse::<u32>().map_err(|e| sup_error!(Error::InvalidPort(e))));
            exposes.push(port_num);
        }
        let service_rumor = ServiceRumor::new(self.butterfly.member_id(),
                                              service_group.clone(),
                                              hostname,
                                              format!("{}", ip),
                                              exposes);
        self.butterfly.insert_service(service_rumor);

        let service = try!(Service::new(service_group, package, topology, update_strategy));
        self.services.push(service);

        Ok(())
    }

    pub fn build_census(&mut self, last_update: &CensusUpdate) -> Result<(bool, CensusUpdate)> {
        let update = CensusUpdate::new(self.butterfly.service_store.get_update_counter(),
                                       self.butterfly.election_store.get_update_counter(),
                                       self.butterfly.member_list.get_update_counter());

        if &update != last_update {
            let mut cl = CensusList::new();
            debug!("Updating census from butterfly data");
            self.butterfly.service_store.with_keys(|(service_group, rumors)| {
                for (member_id, service) in rumors.iter() {
                    let mut ce = CensusEntry::default();
                    ce.populate_from_service(service);
                    cl.insert(String::from(self.butterfly.member_id()), ce);
                }
            });
            self.butterfly.election_store.with_keys(|(service_group, rumors)| {
                // We know you have an election, and this is the only key in the hash
                let election = rumors.get("election").unwrap();
                cl.populate_from_election(election);
            });
            self.butterfly.member_list.with_members(|member| {
                cl.populate_from_member(member);
            });
            self.census_list = cl;
            return Ok((true, update));
        }

        Ok((false, update))
    }

    fn check_for_incoming_signals(&mut self) -> bool {
        match signals::check_for_signal() {
            Some(SignalEvent::Shutdown) => {
                for service in self.services.iter_mut() {
                    outputln!("Shutting down {}", service);
                    service.down()
                        .unwrap_or_else(|err| outputln!("Failed to shutdown {}: {}", service, err));
                }
                true
            }
            Some(SignalEvent::Passthrough(signal_code)) => {
                for service in self.services.iter() {
                    outputln!("Forwarding signal {} to {}", signal_code, service);
                    service.send_signal(signal_code);
                }
                false
            }
            None => false,
        }
    }

    /// Walk each service and check if it has an updated package installed via the Update Strategy.
    pub fn check_for_updated_packages(&mut self) {
        for service in self.services.iter_mut() {
            service.check_for_updated_package();
        }
    }

    //  * Start butterfly
    //  Loop {
    //    * Check for incoming signals; forward them; shut down if neccessary
    //    * Check if each service needs its package updated
    //      * Update the package
    //    * Check if the Census needs building from Butterfly, or the package changed
    //      * Loop the services, and reconfigure the service from the Census
    //    * Reap any dead children
    //    * Start or restart the services
    //  }
    //
    pub fn run(&mut self) -> Result<()> {
        // Set the global signal handlers
        signals::init();

        // Start butterfly
        try!(self.butterfly.start(Timing::default()));

        // Watch for updates
        let mut last_census_update = CensusUpdate::new(0, 0, 0);

        'services: loop {
            // Check for incoming signals.
            //
            // This takes signals passed to `hab-sup` and either shuts down all the services, or
            // passes the signals through. This functionality is totally going to need a refactor
            // when we get all the way to a single-sup-per-kernel model, since passing all random
            // signals through to all services is most certainly not what you want.
            //
            // This function returns true if we are supposed to shut the system down, false if we
            // can keep going.
            if self.check_for_incoming_signals() {
                outputln!("Habitat thanks you - shutting down!");
                return Ok(());
            }

            // Check for updated packages; this updates the Service to point to the new service
            // struct, and then marks it for restarting.
            self.check_for_updated_packages();

            // Try and build the census from the gossip data, updating the last_census_update with
            // the resulting checkpoints. The census is our representation of the data produced
            // by Butterfly.
            let (census_updated, ncu) = try!(self.build_census(&last_census_update));
            last_census_update = ncu;

            // Configure services

            // Reap dead children

            // Start or restart services

        }

        //        'services loop {
        //            self.build_census();
        //
        //            for service in self.services {
        //                service.run();
        //            }
        //        }
        Ok(())
    }
}
