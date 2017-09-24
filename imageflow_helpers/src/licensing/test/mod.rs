mod strings;
mod support;

use self::strings::*;
use self::support::*;
use super::*;
use super::cache::*;
use super::compute::*;
use super::license_pair::*;
use super::parsing::*;
use super::support::*;

use mockito;
use mockito::mock;

use ::smallvec::SmallVec;


//#[cfg(not(test))]
//const URL: &'static str = "https://api.twitter.com";
//
//#[cfg(test)]
//const URL: &'static str = mockito::SERVER_URL;
//
//
//let _m = mock("GET", "/hello")
//.with_status(201)
//.with_header("content-type", "text/plain")
//.with_header("x-api-key", "1234")
//.with_body("world")
//.create();



#[test]
fn test_remote_license_success(){

    let mock = mock("GET", "/").with_status(200).with_header("content-type", "text/plain").with_body(SITE_WIDE_REMOTE).create();

    let clock = Box::new(OffsetClock::new("2017-04-25", "2017-04-25"));
    let cache = StringMemCache::new().into_cache();
    let mut mgr = LicenseManagerSingleton::new(&*parsing::TEST_KEYS, clock, cache);
    mgr.rewind_created_date(60 * 60 * 20);
    let license = mgr.get_or_add(&Cow::Borrowed(SITE_WIDE_PLACEHOLDER)).unwrap();

    mgr.wait();

    let req_features = SmallVec::from_buf(["R_Creative"]);

    let compute = mgr.compute(true, LicenseScope::All,req_features );

    assert!(compute.licensed());

    mock.assert();
}


