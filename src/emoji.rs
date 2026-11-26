//! Built-in emoji, drawn as small SVGs (64x64 viewBox) so overlays work on a server with no emoji font.
//!
//! Lookup key = the code points in lowercase hex joined by `-`, without the `fe0f` variation selector
//! (the same naming Twemoji uses: `1f339`, `1f469-200d-1f4bb`). `:rose:` short names work in overlay
//! text too. Anything not in this table is looked up as `<key>.svg` / `<key>.png` in `--emoji-dir`
//! (drop the Twemoji or Noto set there for full coverage), and otherwise drawn as a placeholder.

/// `(key, short name, svg)`.
pub const SET: &[(&str, &str, &str)] = &[
    ("1f339", "rose", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect x="30" y="34" width="4" height="26" fill="#2e7d32"/><path d="M32 50C40 44 50 48 50 48C46 57 37 57 32 50Z" fill="#43a047"/><path d="M32 44C24 38 14 42 14 42C18 51 27 51 32 44Z" fill="#43a047"/><circle cx="32" cy="22" r="17" fill="#d81b60"/><path d="M32 9C44 9 48 22 41 29C34 36 22 31 25 21C27 14 37 14 37 21" fill="none" stroke="#880e4f" stroke-width="3"/><path d="M20 20C18 28 24 36 32 38" fill="none" stroke="#ad1457" stroke-width="3"/></svg>"##),
    ("2764", "heart", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><path d="M32 58C6 40 2 22 13 13C22 6 30 11 32 19C34 11 42 6 51 13C62 22 58 40 32 58Z" fill="#e53935"/><path d="M14 20C16 15 21 13 25 15" fill="none" stroke="#ff8a80" stroke-width="3" stroke-linecap="round"/></svg>"##),
    ("1f525", "fire", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><path d="M32 3C36 18 53 26 51 42C49 55 41 61 32 61C22 61 13 53 13 40C13 30 22 24 24 13C28 17 28 23 30 26C33 20 30 11 32 3Z" fill="#ff7043"/><path d="M32 30C36 38 43 42 41 51C39 57 35 59 32 59C27 59 23 55 23 49C23 42 30 38 32 30Z" fill="#ffd54f"/></svg>"##),
    ("2604", "comet", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><path d="M36 30L4 6L28 38Z" fill="#ffb74d"/><path d="M30 36L2 26L26 44Z" fill="#ff8a65"/><path d="M40 28L18 4L36 34Z" fill="#ffe082"/><circle cx="44" cy="44" r="15" fill="#6d4c41"/><circle cx="48" cy="40" r="4" fill="#8d6e63"/><circle cx="40" cy="49" r="3" fill="#4e342e"/></svg>"##),
    ("2694", "swords", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><g transform="rotate(45 32 32)"><path d="M29 2L32 0L35 2V40H29Z" fill="#cfd8dc"/><rect x="22" y="40" width="20" height="5" rx="2" fill="#8d6e63"/><rect x="29" y="45" width="6" height="14" fill="#5d4037"/></g><g transform="rotate(-45 32 32)"><path d="M29 2L32 0L35 2V40H29Z" fill="#eceff1"/><rect x="22" y="40" width="20" height="5" rx="2" fill="#a1887f"/><rect x="29" y="45" width="6" height="14" fill="#6d4c41"/></g></svg>"##),
    ("1f6e1", "shield", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><path d="M32 3L57 11V32C57 47 45 57 32 61C19 57 7 47 7 32V11Z" fill="#1e88e5" stroke="#0d47a1" stroke-width="3"/><path d="M32 10L50 16V32C50 43 42 51 32 55Z" fill="#42a5f5"/><rect x="29" y="16" width="6" height="34" fill="#fff"/><rect x="18" y="26" width="28" height="6" fill="#fff"/></svg>"##),
    ("1f3f0", "castle", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect x="8" y="26" width="48" height="32" fill="#9e9e9e"/><rect x="8" y="16" width="12" height="12" fill="#bdbdbd"/><rect x="26" y="16" width="12" height="12" fill="#bdbdbd"/><rect x="44" y="16" width="12" height="12" fill="#bdbdbd"/><rect x="6" y="12" width="6" height="6" fill="#bdbdbd"/><rect x="16" y="12" width="6" height="6" fill="#bdbdbd"/><rect x="42" y="12" width="6" height="6" fill="#bdbdbd"/><rect x="52" y="12" width="6" height="6" fill="#bdbdbd"/><rect x="31" y="2" width="2" height="14" fill="#5d4037"/><path d="M33 2L45 6L33 10Z" fill="#e53935"/><path d="M24 58V44A8 8 0 0 1 40 44V58Z" fill="#5d4037"/><rect x="12" y="34" width="4" height="8" fill="#455a64"/><rect x="48" y="34" width="4" height="8" fill="#455a64"/></svg>"##),
    ("1f451", "crown", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><path d="M5 50L9 17L22 32L32 10L42 32L55 17L59 50Z" fill="#fdd835" stroke="#f9a825" stroke-width="3" stroke-linejoin="round"/><rect x="5" y="48" width="54" height="8" rx="2" fill="#f9a825"/><circle cx="32" cy="38" r="4" fill="#e53935"/><circle cx="17" cy="42" r="3" fill="#29b6f6"/><circle cx="47" cy="42" r="3" fill="#66bb6a"/></svg>"##),
    ("2b50", "star", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><polygon points="32,3 40,23 61,24 45,38 50,59 32,47 14,59 19,38 3,24 24,23" fill="#fdd835" stroke="#f9a825" stroke-width="3" stroke-linejoin="round"/></svg>"##),
    ("1f31f", "glowing_star", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><circle cx="32" cy="32" r="30" fill="#fff59d" opacity="0.45"/><polygon points="32,5 39,24 59,25 44,38 49,57 32,46 15,57 20,38 5,25 25,24" fill="#fdd835" stroke="#f9a825" stroke-width="3" stroke-linejoin="round"/></svg>"##),
    ("1f480", "skull", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><circle cx="32" cy="27" r="22" fill="#eceff1"/><rect x="20" y="38" width="24" height="18" rx="5" fill="#eceff1"/><circle cx="23" cy="28" r="6" fill="#263238"/><circle cx="41" cy="28" r="6" fill="#263238"/><path d="M32 34L28 42H36Z" fill="#263238"/><path d="M26 48V56M32 48V56M38 48V56" stroke="#90a4ae" stroke-width="2"/></svg>"##),
    ("1f3c6", "trophy", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><path d="M16 6H48V26C48 38 40 45 32 45C24 45 16 38 16 26Z" fill="#fdd835" stroke="#f9a825" stroke-width="2"/><path d="M16 12H7C7 24 11 30 19 33" fill="none" stroke="#f9a825" stroke-width="4"/><path d="M48 12H57C57 24 53 30 45 33" fill="none" stroke="#f9a825" stroke-width="4"/><rect x="28" y="45" width="8" height="9" fill="#f9a825"/><rect x="18" y="54" width="28" height="7" rx="2" fill="#8d6e63"/></svg>"##),
    ("26a1", "bolt", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><polygon points="38,2 12,36 28,36 22,62 52,24 35,24" fill="#ffeb3b" stroke="#f9a825" stroke-width="3" stroke-linejoin="round"/></svg>"##),
    ("1f3f9", "bow", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><path d="M20 4C50 16 50 48 20 60" fill="none" stroke="#8d6e63" stroke-width="5" stroke-linecap="round"/><path d="M20 4V60" stroke="#eceff1" stroke-width="2"/><path d="M6 32H54" stroke="#b0bec5" stroke-width="3"/><polygon points="62,32 52,26 52,38" fill="#78909c"/><polygon points="6,32 2,27 12,32 2,37" fill="#e53935"/></svg>"##),
    ("1f381", "gift", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect x="8" y="28" width="48" height="30" fill="#e53935"/><rect x="4" y="18" width="56" height="12" fill="#c62828"/><rect x="28" y="18" width="8" height="40" fill="#fdd835"/><path d="M32 18C20 2 8 10 18 18Z" fill="#fdd835"/><path d="M32 18C44 2 56 10 46 18Z" fill="#f9a825"/></svg>"##),
    ("1f48e", "gem", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><polygon points="16,8 48,8 62,24 32,60 2,24" fill="#29b6f6" stroke="#0277bd" stroke-width="3" stroke-linejoin="round"/><path d="M2 24H62M22 8L32 24L42 8M32 24L20 24L32 60L44 24Z" fill="none" stroke="#e1f5fe" stroke-width="2"/></svg>"##),
    ("1f680", "rocket", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><path d="M32 2C47 14 47 40 43 50H21C17 40 17 14 32 2Z" fill="#eceff1" stroke="#90a4ae" stroke-width="2"/><circle cx="32" cy="24" r="7" fill="#29b6f6" stroke="#0277bd" stroke-width="2"/><path d="M21 38L8 52L21 50Z" fill="#e53935"/><path d="M43 38L56 52L43 50Z" fill="#e53935"/><path d="M25 52H39L32 63Z" fill="#ff9800"/></svg>"##),
    ("1f600", "smile", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><circle cx="32" cy="32" r="29" fill="#ffca28" stroke="#f9a825" stroke-width="2"/><ellipse cx="23" cy="26" rx="4" ry="6" fill="#4e342e"/><ellipse cx="41" cy="26" rx="4" ry="6" fill="#4e342e"/><path d="M15 38Q32 58 49 38Z" fill="#5d4037"/><path d="M22 44Q32 50 42 44" fill="#e57373"/></svg>"##),
    ("1f602", "joy", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><circle cx="32" cy="32" r="29" fill="#ffca28" stroke="#f9a825" stroke-width="2"/><path d="M14 26Q20 18 27 25M37 25Q44 18 50 26" fill="none" stroke="#4e342e" stroke-width="3" stroke-linecap="round"/><path d="M13 36Q32 62 51 36Z" fill="#5d4037"/><path d="M8 28Q4 38 10 40Q14 34 12 28ZM56 28Q60 38 54 40Q50 34 52 28Z" fill="#4fc3f7"/></svg>"##),
    ("1f60d", "heart_eyes", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><circle cx="32" cy="32" r="29" fill="#ffca28" stroke="#f9a825" stroke-width="2"/><path d="M23 36C10 28 12 18 19 18C22 18 23 21 23 22C23 21 24 18 27 18C34 18 36 28 23 36Z" fill="#e53935"/><path d="M42 36C29 28 31 18 38 18C41 18 42 21 42 22C42 21 43 18 46 18C53 18 55 28 42 36Z" fill="#e53935"/><path d="M18 42Q32 58 46 42Z" fill="#5d4037"/></svg>"##),
    ("1f621", "angry", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><circle cx="32" cy="32" r="29" fill="#ef5350" stroke="#c62828" stroke-width="2"/><path d="M13 20L28 27M51 20L36 27" stroke="#3e2723" stroke-width="4" stroke-linecap="round"/><circle cx="23" cy="32" r="4" fill="#3e2723"/><circle cx="41" cy="32" r="4" fill="#3e2723"/><path d="M20 50Q32 38 44 50" fill="none" stroke="#3e2723" stroke-width="4" stroke-linecap="round"/></svg>"##),
    ("1f60e", "cool", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><circle cx="32" cy="32" r="29" fill="#ffca28" stroke="#f9a825" stroke-width="2"/><rect x="9" y="20" width="21" height="14" rx="5" fill="#263238"/><rect x="34" y="20" width="21" height="14" rx="5" fill="#263238"/><rect x="29" y="23" width="6" height="4" fill="#263238"/><path d="M18 44Q32 56 46 44" fill="none" stroke="#5d4037" stroke-width="4" stroke-linecap="round"/></svg>"##),
    ("1f389", "party", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><polygon points="4,60 20,20 44,44" fill="#ffb300"/><polygon points="4,60 12,40 24,52" fill="#ff7043"/><circle cx="40" cy="14" r="3" fill="#e53935"/><circle cx="54" cy="26" r="3" fill="#29b6f6"/><circle cx="50" cy="8" r="2.5" fill="#66bb6a"/><circle cx="58" cy="44" r="3" fill="#ab47bc"/><path d="M30 24Q40 18 36 8M36 34Q46 34 52 28M26 40Q34 50 48 46" fill="none" stroke="#fdd835" stroke-width="3" stroke-linecap="round"/></svg>"##),
    ("1f4a5", "boom", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><polygon points="32,2 38,18 54,8 46,24 62,28 46,36 58,52 40,46 38,62 30,48 16,60 20,42 2,40 18,32 4,18 22,22" fill="#ff7043"/><polygon points="32,14 36,26 46,20 41,30 52,34 40,38 46,46 35,42 32,52 27,42 18,48 23,38 12,34 24,30 17,20 28,26" fill="#ffd54f"/></svg>"##),
    ("2728", "sparkles", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><path d="M26 6Q29 26 48 29Q29 32 26 52Q23 32 4 29Q23 26 26 6Z" fill="#fdd835"/><path d="M50 30Q51 40 60 42Q51 44 50 54Q49 44 40 42Q49 40 50 30Z" fill="#ffee58"/><path d="M48 4Q49 10 54 11Q49 12 48 18Q47 12 42 11Q47 10 48 4Z" fill="#fff59d"/></svg>"##),
    ("1faa8", "rock", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><polygon points="6,52 12,28 30,14 50,20 60,40 54,56" fill="#78909c" stroke="#455a64" stroke-width="2" stroke-linejoin="round"/><path d="M12 28L30 34L50 20M30 34L28 56M30 34L60 40" fill="none" stroke="#546e7a" stroke-width="2"/></svg>"##),
    ("1f338", "blossom", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><g fill="#f48fb1"><circle cx="32" cy="14" r="12"/><circle cx="50" cy="27" r="12"/><circle cx="43" cy="48" r="12"/><circle cx="21" cy="48" r="12"/><circle cx="14" cy="27" r="12"/></g><circle cx="32" cy="32" r="9" fill="#fdd835"/></svg>"##),
    ("1f319", "moon", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><path d="M40 4A28 28 0 1 0 60 42A22 22 0 1 1 40 4Z" fill="#fdd835" stroke="#f9a825" stroke-width="2"/></svg>"##),
    ("2600", "sun", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><g stroke="#ffb300" stroke-width="5" stroke-linecap="round"><path d="M32 3V12M32 52V61M3 32H12M52 32H61M11 11L17 17M47 47L53 53M53 11L47 17M17 47L11 53"/></g><circle cx="32" cy="32" r="16" fill="#ffca28" stroke="#ffb300" stroke-width="3"/></svg>"##),
    ("1f4a3", "bomb", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><circle cx="28" cy="40" r="21" fill="#37474f"/><circle cx="21" cy="32" r="5" fill="#78909c" opacity="0.7"/><rect x="38" y="12" width="9" height="10" rx="2" transform="rotate(35 42 17)" fill="#8d6e63"/><path d="M46 12Q54 6 58 10" fill="none" stroke="#a1887f" stroke-width="3"/><polygon points="58,2 60,8 66,8 61,12 63,18 58,14 53,18 55,12 50,8 56,8" fill="#ff7043" transform="translate(-6 0)"/></svg>"##),
    ("1f5e1", "dagger", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><g transform="rotate(45 32 32)"><path d="M32 0L38 12V40H26V12Z" fill="#cfd8dc" stroke="#90a4ae" stroke-width="1.5"/><rect x="20" y="40" width="24" height="5" rx="2" fill="#fbc02d"/><rect x="29" y="45" width="6" height="12" fill="#5d4037"/><circle cx="32" cy="60" r="3" fill="#fbc02d"/></g></svg>"##),
    ("1f3af", "target", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><circle cx="32" cy="32" r="29" fill="#e53935"/><circle cx="32" cy="32" r="21" fill="#fff"/><circle cx="32" cy="32" r="13" fill="#e53935"/><circle cx="32" cy="32" r="5" fill="#fff"/></svg>"##),
    ("1f4b0", "money", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><path d="M22 14L25 4H39L42 14C55 24 59 40 52 51C47 59 17 59 12 51C5 40 9 24 22 14Z" fill="#a1887f"/><rect x="21" y="10" width="22" height="6" rx="3" fill="#6d4c41"/><circle cx="32" cy="38" r="12" fill="#fdd835" stroke="#f9a825" stroke-width="2"/><path d="M32 30V46M27 34Q32 30 37 34Q27 40 37 44Q32 48 27 44" fill="none" stroke="#f9a825" stroke-width="2.5"/></svg>"##),
    ("1f340", "clover", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><g fill="#43a047"><circle cx="22" cy="22" r="13"/><circle cx="42" cy="22" r="13"/><circle cx="22" cy="42" r="13"/><circle cx="42" cy="42" r="13"/></g><path d="M32 32Q34 52 46 62" fill="none" stroke="#2e7d32" stroke-width="4" stroke-linecap="round"/></svg>"##),
    ("1f308", "rainbow", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><g fill="none" stroke-width="6"><path d="M4 56A28 28 0 0 1 60 56" stroke="#e53935"/><path d="M10 56A22 22 0 0 1 54 56" stroke="#fb8c00"/><path d="M16 56A16 16 0 0 1 48 56" stroke="#fdd835"/><path d="M22 56A10 10 0 0 1 42 56" stroke="#43a047"/></g></svg>"##),
    ("2744", "snow", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><g stroke="#4fc3f7" stroke-width="5" stroke-linecap="round"><path d="M32 4V60M8 18L56 46M56 18L8 46"/></g><g stroke="#81d4fa" stroke-width="3" stroke-linecap="round"><path d="M26 10L32 16L38 10M26 54L32 48L38 54"/></g></svg>"##),
    ("1f30a", "wave", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><path d="M2 44C2 20 24 6 46 14C34 16 30 26 38 32C46 38 58 34 62 28C62 52 46 62 28 60C12 58 2 54 2 44Z" fill="#29b6f6"/><path d="M8 48Q24 40 34 52Q44 56 56 46" fill="none" stroke="#e1f5fe" stroke-width="3" stroke-linecap="round"/></svg>"##),
    ("1f6a9", "flag", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect x="10" y="4" width="5" height="58" fill="#6d4c41"/><path d="M15 6C28 0 36 14 54 8V34C36 40 28 28 15 34Z" fill="#e53935"/></svg>"##),
    ("1f3f3", "white_flag", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect x="10" y="4" width="5" height="58" fill="#6d4c41"/><path d="M15 6C28 0 36 14 54 8V34C36 40 28 28 15 34Z" fill="#fafafa" stroke="#bdbdbd" stroke-width="2"/></svg>"##),
    ("1f534", "red_circle", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><circle cx="32" cy="32" r="28" fill="#e53935"/><circle cx="24" cy="22" r="7" fill="#ff8a80" opacity="0.6"/></svg>"##),
    ("1f535", "blue_circle", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><circle cx="32" cy="32" r="28" fill="#1e88e5"/><circle cx="24" cy="22" r="7" fill="#90caf9" opacity="0.6"/></svg>"##),
    ("1f7e2", "green_circle", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><circle cx="32" cy="32" r="28" fill="#43a047"/></svg>"##),
    ("1f7e1", "yellow_circle", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><circle cx="32" cy="32" r="28" fill="#fdd835"/></svg>"##),
    ("1f7e5", "red_square", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect x="4" y="4" width="56" height="56" rx="10" fill="#e53935"/></svg>"##),
    ("1f7e6", "blue_square", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect x="4" y="4" width="56" height="56" rx="10" fill="#1e88e5"/></svg>"##),
    ("2705", "check", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect x="4" y="4" width="56" height="56" rx="12" fill="#43a047"/><path d="M16 33L28 45L48 20" fill="none" stroke="#fff" stroke-width="8" stroke-linecap="round" stroke-linejoin="round"/></svg>"##),
    ("274c", "cross", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><path d="M12 12L52 52M52 12L12 52" stroke="#e53935" stroke-width="10" stroke-linecap="round"/></svg>"##),
    ("26a0", "warning", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><path d="M32 4L62 58H2Z" fill="#fdd835" stroke="#f9a825" stroke-width="3" stroke-linejoin="round"/><rect x="29" y="22" width="6" height="20" rx="2" fill="#3e2723"/><circle cx="32" cy="49" r="3.5" fill="#3e2723"/></svg>"##),
    ("27a1", "right", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><path d="M6 24H34V8L60 32L34 56V40H6Z" fill="#1e88e5"/></svg>"##),
    ("2b05", "left", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><path d="M58 24H30V8L4 32L30 56V40H58Z" fill="#1e88e5"/></svg>"##),
    ("1f947", "gold", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><path d="M16 2L28 26H36L48 2Z" fill="#e53935"/><circle cx="32" cy="42" r="19" fill="#fdd835" stroke="#f9a825" stroke-width="3"/><path d="M28 36L33 32V52" fill="none" stroke="#f9a825" stroke-width="5" stroke-linecap="round"/></svg>"##),
    ("1f948", "silver", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><path d="M16 2L28 26H36L48 2Z" fill="#1e88e5"/><circle cx="32" cy="42" r="19" fill="#cfd8dc" stroke="#90a4ae" stroke-width="3"/><path d="M24 36Q32 26 40 36Q40 42 24 52H40" fill="none" stroke="#78909c" stroke-width="4" stroke-linecap="round"/></svg>"##),
    ("1f949", "bronze", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><path d="M16 2L28 26H36L48 2Z" fill="#43a047"/><circle cx="32" cy="42" r="19" fill="#d7885a" stroke="#a65d33" stroke-width="3"/><path d="M24 34H38L30 42Q42 42 40 50Q36 55 24 52" fill="none" stroke="#8d4a22" stroke-width="4" stroke-linecap="round" stroke-linejoin="round"/></svg>"##),
    ("1f981", "lion", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><circle cx="32" cy="32" r="30" fill="#ef6c00"/><circle cx="32" cy="34" r="20" fill="#ffca28"/><circle cx="24" cy="30" r="3" fill="#3e2723"/><circle cx="40" cy="30" r="3" fill="#3e2723"/><path d="M27 40H37L32 46Z" fill="#6d4c41"/><path d="M32 46V50M26 52Q32 50 32 50Q32 50 38 52" fill="none" stroke="#6d4c41" stroke-width="2.5" stroke-linecap="round"/></svg>"##),
    ("1f432", "dragon", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><path d="M10 6L22 18L14 22ZM54 6L42 18L50 22Z" fill="#fdd835"/><circle cx="32" cy="36" r="26" fill="#66bb6a" stroke="#388e3c" stroke-width="2"/><ellipse cx="32" cy="46" rx="14" ry="10" fill="#a5d6a7"/><circle cx="26" cy="46" r="2.5" fill="#2e7d32"/><circle cx="38" cy="46" r="2.5" fill="#2e7d32"/><circle cx="21" cy="30" r="5" fill="#fff"/><circle cx="43" cy="30" r="5" fill="#fff"/><circle cx="22" cy="31" r="2.5" fill="#d32f2f"/><circle cx="42" cy="31" r="2.5" fill="#d32f2f"/></svg>"##),
    ("1f44d", "thumbs_up", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect x="4" y="28" width="14" height="32" rx="3" fill="#1e88e5"/><path d="M22 30L34 6C42 6 46 14 42 24H58C63 24 63 32 58 36C63 39 61 45 56 47C58 51 54 55 50 55H22Z" fill="#ffca28" stroke="#f9a825" stroke-width="2" stroke-linejoin="round"/></svg>"##),
    ("1f9f1", "brick", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect x="2" y="10" width="60" height="44" fill="#c0622d"/><path d="M2 24.5H62M2 39.5H62M22 10V24.5M42 24.5V39.5M22 39.5V54" stroke="#e8c4a5" stroke-width="3"/></svg>"##),
    ("1f514", "bell", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><path d="M32 4C24 4 18 10 18 18C18 34 12 40 8 46H56C52 40 46 34 46 18C46 10 40 4 32 4Z" fill="#fdd835" stroke="#f9a825" stroke-width="2.5"/><ellipse cx="32" cy="52" rx="8" ry="6" fill="#f9a825"/></svg>"##),
];

/// Strips `fe0f`, lowercases and joins code points: `"🌹"` -> `"1f339"`.
pub fn key_of(cluster: &str) -> String {
    cluster
        .chars()
        .map(|c| c as u32)
        .filter(|&c| c != 0xfe0f)
        .map(|c| format!("{c:x}"))
        .collect::<Vec<_>>()
        .join("-")
}

pub fn by_key(key: &str) -> Option<&'static str> {
    SET.iter().find(|(k, _, _)| *k == key).map(|(_, _, svg)| *svg)
}

pub fn by_name(name: &str) -> Option<(&'static str, &'static str)> {
    let name = name.to_ascii_lowercase();
    SET.iter().find(|(_, n, _)| *n == name).map(|(k, _, svg)| (*k, *svg))
}

/// True for characters that start (or belong to) an emoji cluster.
pub fn is_emoji_char(c: char) -> bool {
    matches!(c as u32,
        0x1f300..=0x1faff | 0x2600..=0x27bf | 0x2b00..=0x2bff | 0x2190..=0x21ff | 0x2300..=0x23ff |
        0x1f000..=0x1f2ff | 0x3030 | 0x303d | 0x3297 | 0x3299 | 0xa9 | 0xae | 0x203c | 0x2049 | 0x2122 | 0x2139)
}

/// Splits a string into plain text and emoji clusters (modifier, ZWJ and variation-selector aware).
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Piece {
    Text(String),
    Emoji(String),
}

pub fn split(s: &str) -> Vec<Piece> {
    let chars: Vec<char> = s.chars().collect();
    let mut out: Vec<Piece> = vec![];
    let mut text = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        // :short_name:
        if c == ':' {
            if let Some(end) = chars[i + 1..].iter().position(|&x| x == ':') {
                let name: String = chars[i + 1..i + 1 + end].iter().collect();
                if !name.is_empty() && name.chars().all(|x| x.is_ascii_alphanumeric() || x == '_') {
                    if let Some((key, _)) = by_name(&name) {
                        if !text.is_empty() {
                            out.push(Piece::Text(std::mem::take(&mut text)));
                        }
                        let ch = key.split('-').filter_map(|h| u32::from_str_radix(h, 16).ok()).filter_map(char::from_u32).collect::<String>();
                        out.push(Piece::Emoji(ch));
                        i += end + 2;
                        continue;
                    }
                }
            }
        }
        // ascii digits/#/* are emoji only as keycaps, which we do not draw
        if is_emoji_char(c) {
            let mut cluster = String::new();
            cluster.push(c);
            i += 1;
            while i < chars.len() {
                let n = chars[i];
                let n_u = n as u32;
                if n_u == 0xfe0f || (0x1f3fb..=0x1f3ff).contains(&n_u) || n_u == 0x20e3 {
                    cluster.push(n);
                    i += 1;
                } else if n_u == 0x200d && i + 1 < chars.len() && is_emoji_char(chars[i + 1]) {
                    cluster.push(n);
                    cluster.push(chars[i + 1]);
                    i += 2;
                } else {
                    break;
                }
            }
            if !text.is_empty() {
                out.push(Piece::Text(std::mem::take(&mut text)));
            }
            out.push(Piece::Emoji(cluster));
        } else {
            text.push(c);
            i += 1;
        }
    }
    if !text.is_empty() {
        out.push(Piece::Text(text));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_names() {
        assert_eq!(key_of("🌹"), "1f339");
        assert_eq!(key_of("❤️"), "2764");
        assert!(by_key("1f339").is_some());
        assert_eq!(by_name("Rose").map(|x| x.0), Some("1f339"));
        let mut keys: Vec<_> = SET.iter().map(|s| s.0).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), SET.len(), "duplicate emoji key");
    }

    #[test]
    fn splits_text_emoji_and_shortcodes() {
        let p = split("Budi 🌹x3 :fire: ok");
        assert_eq!(
            p,
            vec![Piece::Text("Budi ".into()), Piece::Emoji("🌹".into()), Piece::Text("x3 ".into()), Piece::Emoji("🔥".into()), Piece::Text(" ok".into())]
        );
        assert_eq!(split("a:b:c"), vec![Piece::Text("a:b:c".into())]);
        assert_eq!(split("❤️"), vec![Piece::Emoji("❤️".into())]);
    }
}
