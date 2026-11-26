# tiktok-live

Vendored from [PirateTok/live-rs](https://github.com/PirateTok/live-rs) 0.5.0 (BSD Zero Clause License, see `LICENSE`),
with these changes:

* `build.rs`, the binaries, examples and tests were left out.
* The anonymous `ttwid` cookie is asked from `https://www.tiktok.com/live` first and `https://www.tiktok.com/` second
  (`TikTokLiveConfig::ttwid_urls`). TikTok stopped setting the cookie on the home page, which made every connection
  fail after "room id resolved" with `no ttwid cookie after 8 attempts`.
* The `openssl` crate is built from source on Linux (`vendored`), so the release binary needs no particular libssl.

It uses TikTok's private web protocol, which is not an official API. TikTok can change it at any time.
