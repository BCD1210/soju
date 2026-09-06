// Runs only inside the signed-in user's Steam page. No cookies or tokens leave WebKit.
if (location.protocol !== "https:" || !["steamcommunity.com", "store.steampowered.com"].includes(location.hostname)) {
    return { state: "wrong-page" };
}
const config = document.getElementById("application_config");
const attribute = name => {
    try { return JSON.parse(config?.getAttribute(name) || "{}"); } catch { return {}; }
};
const user = attribute("data-userinfo");
const steamid = String(window.g_steamID || user.steamid || "");
if (!/^765[0-9]{14}$/.test(steamid)) return { state: "sign-in" };
if (user.steamid && String(user.steamid) !== steamid) return { state: "unavailable" };

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

// The full All Games page carries names for unavailable/delisted titles as well.
const profile = String(window.g_rgProfileData?.steamid || "");
if (location.hostname === "steamcommunity.com" && /\/games\/?$/.test(location.pathname) &&
    new URLSearchParams(location.search).get("tab") === "all" &&
    profile === steamid && Array.isArray(window.rgGames)) {
    return snapshot(window.rgGames, window.rgGames.length);
}

// Modern Steam pages use the website's own authenticated Web API session.
// Keep it within this page's JS context; return only game IDs and names to Soju.
const token = user.webapi_token;
if (user.logged_in !== true || typeof token !== "string" || !token) return { state: "open-library" };
const controller = new AbortController();
const timer = setTimeout(() => controller.abort(), 20000);
try {
    const params = new URLSearchParams({
        access_token: token,
        input_json: JSON.stringify({ steamid, include_appinfo: true, include_played_free_games: true })
    });
    const response = await fetch("https://api.steampowered.com/IPlayerService/GetOwnedGames/v1/?" + params, {
        signal: controller.signal, redirect: "error", credentials: "omit", cache: "no-store"
    });
    if (!response.ok) return { state: "unavailable" };
    const raw = await response.text();
    if (raw.length > 8 * 1024 * 1024) return { state: "unavailable" };
    const value = JSON.parse(raw).response;
    if (!value || !Object.hasOwn(value, "game_count")) return { state: "unavailable" };
    return snapshot(value.games || [], value.game_count);
} catch {
    return { state: "unavailable" };
} finally {
    clearTimeout(timer);
}
