use ::preludes::from_std::*;
use ::std;
use num::{One, Zero};
use num::bigint::{BigInt, Sign};
use sha2::{Sha512, Digest};
use ::chrono::{DateTime,FixedOffset};
use unicase::UniCase;
use ::app_dirs::*;
use errors::*;
use errors::Result;
use ::lockless::primitives::append_list::AppendList;
use lockless::primitives::append_list::AppendListIterator;
use std::ascii::AsciiExt;
use chrono::Utc;
use std::thread;
use std::sync::mpsc::channel;
use std::thread::JoinHandle;


// Get build date
// Get ticks
// Get utcnow
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

mod cache;
mod parsing;
mod compute;
mod support;
mod license_pair;

#[cfg(test)]
mod test;

use self::license_pair::*;
use self::support::*;
use self::cache::*;
use self::parsing::*;
use self::compute::*;
// IssueSink

pub trait LicenseClock: Sync + Send{
    fn get_timestamp_ticks(&self) -> u64;
    fn ticks_per_second(&self) -> u64;
    fn get_build_date(&self) -> DateTime<FixedOffset>;
    fn get_utc_now(&self) -> DateTime<Utc>;
}


pub struct LicenseManagerSingleton{
    licenses: AppendList<License>,
    aliases_to_id: ::chashmap::CHashMap<Cow<'static, str>,Cow<'static, str>>,
    cached: ::chashmap::CHashMap<Cow<'static, str>,LicenseBlob>,
    sink: IssueSink,
    trusted_keys: &'static [RSADecryptPublic],
    cache: Box<PersistentStringCache>,
    created: DateTime<Utc>,
    uid: ::uuid::Uuid,
    heartbeat_count: AtomicU64,
    clock: Box<LicenseClock>,
    handle: Arc<::parking_lot::RwLock<Option<JoinHandle<()>>>>
}

#[cfg(not(test))]
const URL: &'static str = "https://licenses-redirect.imazen.net";

#[cfg(test)]
const URL: &'static str = ::mockito::SERVER_URL;


impl LicenseManagerSingleton{
    pub fn new(trusted_keys: &'static [RSADecryptPublic], clock: Box<LicenseClock + Sync>, cache: Box<PersistentStringCache>) -> Self{
        LicenseManagerSingleton{
            trusted_keys,
            clock,
            cache,
            cached: ::chashmap::CHashMap::new(),
            aliases_to_id: ::chashmap::CHashMap::new(),
            licenses: AppendList::new(),
            sink: IssueSink::new("LicenseManager"),
            created: ::chrono::Utc::now(),
            uid: ::uuid::Uuid::new_v4(),
            heartbeat_count: ::std::sync::atomic::ATOMIC_U64_INIT,
            handle: Arc::new(::parking_lot::RwLock::new(None)),
        }

    }
    #[cfg(test)]
    pub fn rewind_created_date(&mut self, seconds: i64){
        self.created = self.created.checked_sub_signed(::chrono::Duration::seconds(seconds)).unwrap();
    }
    fn set_handle(&self, h: Option<JoinHandle<()>>){
        *self.handle.write() = h
    }


    pub fn create_thread(mgr: Arc<LicenseManagerSingleton>){
        let clone = mgr.clone();
        let handle = thread::spawn(move || {
            let _ = mgr.created();
            let client = ::reqwest::Client::new().unwrap();
            for license in mgr.iter_all() {
                if let &License::Pair(ref p) = license{
                    let url = format!("{}/v1/licenses/latest/{}.txt",URL, p.secret());
                    let mut response = client.get(&url).send().unwrap();
                    if response.status().is_success(){
                        let mut buf = Vec::new();
                        response.read_to_end(&mut buf);
                        let s = ::std::str::from_utf8(&buf).unwrap();
                        let blob = LicenseBlob::deserialize(mgr.trusted_keys, s, "remote license").unwrap();
                        p.update_remote(blob);
                    }
                }
            }
            ()
        });
        clone.set_handle(Some(handle));
    }

    pub fn wait(&self){
        let read = self.handle.read();
        read.as_ref().unwrap().join().unwrap();
        //Wait for requests to complete
    }
    pub fn clock(&self) -> &LicenseClock{
        &*self.clock
    }
    pub fn created(&self) -> DateTime<Utc>{
        self.created
    }

    pub fn heartbeat(&self){
        let _ = self.heartbeat_count.fetch_add(1, Ordering::Relaxed);
        for l in self.licenses.iter(){
            //trigger heartbeat
        }
    }

    pub fn get_by_id(&self, id: &str) -> Option<&License>{
        self.licenses.iter().find(|l| l.id().eq_ignore_ascii_case(id))
    }

    pub fn cached_remote(&self, id: &str) -> Option<::chashmap::ReadGuard<Cow<'static,str>,LicenseBlob>>{

        //
        self.cached.get(&Cow::Owned(id.to_owned()))

    }
    fn get_by_alias(&self, license: &Cow<'static, str>) -> Option<&License>{
        if let Some(id) = self.aliases_to_id.get(license){
            if let Some(lic) = self.get_by_id(license.as_ref()){
                return Some(lic);
            }
        }
        None
    }

    pub fn get_or_add(&self, license: &Cow<'static, str>) -> Result<&License>{
        if let Some(lic) = self.get_by_alias(license){
            return Ok(lic)
        }

        // License parsing involves several dozen allocations.
        // Not cheap; thus the aliases_to_id table
        let parsed = LicenseBlob::deserialize(self.trusted_keys, license.as_ref(), "local license")?;

        let id = parsed.fields().id().to_owned();

        self.aliases_to_id.insert(license.clone(), Cow::Owned(id.to_owned()));

        if let Some(lic) = self.get_by_id(&id){
            Ok(lic)
        }else{
            self.add_license(parsed)
        }
    }

    fn add_license(&self, parsed: LicenseBlob) -> Result<&License>{
        let id_copy = parsed.fields().id().to_owned();
        self.licenses.append(License::new(parsed)?);
        // This ensures that we never access duplicates (which can be created in race conditions)
        Ok(self.get_by_id(&id_copy).expect("License was just appended. Have atomics failed?"))
    }

    pub fn iter_all(&self) -> AppendListIterator<License>{
        self.licenses.iter()
    }
    pub fn iter_shared(&self) -> AppendListIterator<License>{
        self.licenses.iter()
    }

    pub fn compute(&self, enforced: bool,
                   scope: LicenseScope, required_features: ::smallvec::SmallVec<[&str;1]>) -> LicenseComputation{
        LicenseComputation::new(self,enforced, scope, required_features)
    }

}

