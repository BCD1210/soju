// Runs only inside the user's official Steam page. Sessions never leave WebKit.
if (location.protocol !== "https:" || !["steamcommunity.com", "store.steampowered.com"].includes(location.hostname)) {
    return { state: "wrong-page" };
}
const config = document.getElementById("application_config");
const attribute = name => {
    try { return JSON.parse(config?.getAttribute(name) || "{}"); } catch { return {}; }
};
const user = attribute("data-userinfo");
const validID = value => /^765[0-9]{14}$/.test(String(value || ""));
let steamid = String(window.g_steamID || user.steamid || "");
if (validID(user.steamid) && validID(steamid) && String(user.steamid) !== steamid) {
    return { state: "unavailable" };
}

function snapshot(rows, count) {
    if (!Array.isArray(rows) || !Number.isInteger(count) || count < 0 ||
        count > 20000 || rows.length !== count) return { state: "unavailable" };
    const games = [];
    const seen = new Set();
    for (const row of rows) {
        if (!row || !/^[1-9][0-9]{0,9}$/.test(String(row.appid)) ||
            typeof row.name !== "string" || !row.name.trim() || row.name.length > 1000) {
            return { state: "unavailable" };
        }
        const appid = String(row.appid);
        if (seen.has(appid)) return { state: "unavailable" };
        seen.add(appid);
        games.push({ appid, name: row.name.trim().slice(0, 300) });
    }
    return { state: "ready", steamid, game_count: games.length, games };
}

// Legacy All Games pages do not consistently include application_config or a
// webapi_token. Read their complete list only when its owner matches this session.
const profile = String(window.g_rgProfileData?.steamid || "");
const ownAllGames = () => validID(steamid) && location.hostname === "steamcommunity.com" &&
    /\/games\/?$/.test(location.pathname) &&
    new URLSearchParams(location.search).get("tab") === "all" && profile === steamid &&
    Array.isArray(window.rgGames);
if (ownAllGames()) return snapshot(window.rgGames, window.rgGames.length);
if (validID(steamid) && validID(profile) && !ownAllGames() && !user.webapi_token) {
    return { state: "open-library" };
}
const signedInPage = !!document.querySelector?.("#account_pulldown");

const controller = new AbortController();
const timer = setTimeout(() => controller.abort(), 20000);
async function readJSON(url, credentials, limit) {
    const response = await fetch(url, {
        signal: controller.signal, redirect: "error", credentials, cache: "no-store"
    });
    if (!response.ok) throw Error("request failed");
    const raw = await response.text();
    if (raw.length > limit) throw Error("response too large");
    return JSON.parse(raw);
}
try {
    let token = user.logged_in === true && typeof user.webapi_token === "string" ? user.webapi_token : "";
    // Steam's legacy community pages can omit both identity and the token.
    // Ask Steam's own session endpoint on the same origin; do not scrape cookies.
    const loginPage = /^\/(login|openid)(\/|$)/.test(location.pathname);
    if (!token && location.hostname === "steamcommunity.com" && !loginPage &&
        (validID(steamid) || user.logged_in === true || signedInPage)) {
        const session = await readJSON("https://steamcommunity.com/chat/clientjstoken", "same-origin", 64 * 1024);
        if (session.logged_in !== true || !validID(session.steamid)) {
            return { state: validID(steamid) ? "open-library" : "sign-in" };
        }
        if (validID(steamid) && steamid !== String(session.steamid)) return { state: "unavailable" };
        steamid = String(session.steamid);
        if (ownAllGames()) return snapshot(window.rgGames, window.rgGames.length);
        token = typeof session.token === "string" ? session.token : "";
    }
    if (!validID(steamid)) return { state: "sign-in" };
    if (!token) return { state: "open-library" };
    const params = new URLSearchParams({
        access_token: token,
        input_json: JSON.stringify({ steamid, include_appinfo: true, include_played_free_games: true })
    });
    const payload = await readJSON("https://api.steampowered.com/IPlayerService/GetOwnedGames/v1/?" + params,
                                   "omit", 8 * 1024 * 1024);
    const value = payload.response;
    if (!value || !Object.prototype.hasOwnProperty.call(value, "game_count")) return { state: "unavailable" };
    return snapshot(value.games || [], value.game_count);
} catch {
    return { state: "unavailable" };
} finally {
    clearTimeout(timer);
}
