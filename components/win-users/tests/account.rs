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

extern crate habitat_win_users;
extern crate winapi;

use std::env;

use winapi::winnt::{SidTypeUser, SidTypeWellKnownGroup};

use habitat_win_users::account::Account;

#[test]
fn real_account_returns_some() {
  assert_eq!(Account::from_name("Administrator".to_string()).is_some(), true)
}

#[test]
fn bogus_account_returns_none() {
  assert_eq!(Account::from_name("bogus".to_string()).is_none(), true)
}

#[test]
fn user_account_returns_user_type() {
  let acct_type = Account::from_name("Administrator".to_string()).unwrap().account_type;
  assert_eq!(acct_type, SidTypeUser)
}

#[test]
fn local_user_account_returns_local_machine_as_domain() {
  let acct_type = Account::from_name("Administrator".to_string()).unwrap().domain;
  assert_eq!(acct_type, env::var("COMPUTERNAME").unwrap())
}

#[test]
fn well_known_group_account_returns_correct_type() {
  let acct_type = Account::from_name("Everyone".to_string()).unwrap().account_type;
  assert_eq!(acct_type, SidTypeWellKnownGroup)
}
