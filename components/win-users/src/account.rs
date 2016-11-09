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

use std::ptr::null_mut;
use std::io::Error;

use widestring::WideCString;
use winapi::{LPCWSTR, BOOL, PSID, PSID_NAME_USE, LPDWORD};
use winapi::winerror;

use super::sid::Sid;

extern "system" {
  fn LookupAccountNameW(
      lpSystemName: LPCWSTR, lpAccountName: LPCWSTR, Sid: PSID, 
      cbSid: LPDWORD, ReferencedDomainName: LPCWSTR,
      cchReferencedDomainName: LPDWORD, peUse: PSID_NAME_USE,
  ) -> BOOL;
}

pub struct Account {
  pub name: String,
  pub system_name: Option<String>,
  pub domain: String,
  pub account_type: PSID_NAME_USE,
  pub sid: Sid
}

impl Account {
  pub fn from_name(name: String) -> Option<Account> {
    lookup_account(name, None)
  }

  pub fn from_name_and_system(name: String, system_name: String) -> Option<Account> {
    lookup_account(name, Some(system_name))
  }
}

fn lookup_account(name: String, system_name: Option<String>) -> Option<Account> {
  let mut sid_size: u32 = 0;
  let mut domain_size: u32 = 0;
  let wide = WideCString::from_str(name).unwrap();
  unsafe {
    LookupAccountNameW(null_mut(), wide.as_ptr(), null_mut(), &mut sid_size as LPDWORD, null_mut(), &mut domain_size as LPDWORD, null_mut())
  };
  match Error::last_os_error().raw_os_error() {
      Some(ERROR_INSUFFICIENT_BUFFER) => {}
      Some(ERROR_NONE_MAPPED) => return None
      Some(err) => panic!("Error while looking up account for {}: {}", name, Error::last_os_error()),
      None => {} //this will never happen
  }

  let sid: PSID = Vec::with_capacity(sid_size as usize).as_mut_ptr();
  let domain = Vec::with_capacity(domain_size as usize).as_mut_ptr();
  let sid_type: PSID_NAME_USE = Vec::with_capacity(4).as_mut_ptr();

  let ret = unsafe {
    LookupAccountNameW(null_mut(), wide.as_ptr(), sid, &mut sid_size as LPDWORD, domain, &mut domain_size as LPDWORD, sid_type)
  };
  if ret == 0 {
    panic!("Failed to retrieve SID for {}: {}", name, Error::last_os_error());
  }

  let domain_str = unsafe { WideCString::from_ptr_str(domain).to_string_lossy() };
  Some(Account {name: name, system_name: system_name, domain: domain_str, account_type: sid_type, sid: Sid {raw: sid}})
}
