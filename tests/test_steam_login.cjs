const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
const code = fs.readFileSync(path.join(__dirname, "../resources/steam-library.js"), "utf8");
const run = new AsyncFunction("window", "document", "location", "fetch", code);
const id = "76561198000000000";
function context(user = {}, window = {}, url = "https://steamcommunity.com/my/games/?tab=all") {
    return [window, { getElementById: () => ({ getAttribute: key => JSON.stringify(key === "data-userinfo" ? user : {}) }) }, new URL(url)];
}
test("signed-out and foreign pages never request account data", async () => {
    const fetch = () => { throw Error("must not fetch"); };
    assert.deepEqual(await run(...context(), fetch), {state:"sign-in"});
    assert.deepEqual(await run(...context({}, {}, "https://steamcommunity.com.evil.test/"), fetch), {state:"wrong-page"});
});
test("the own All Games page returns only metadata", async () => {
    const w = {g_steamID:id, g_rgProfileData:{steamid:id}, rgGames:[{appid:10, name:"  Owned  ", auth:"do-not-export"}]};
    const value = await run(...context({}, w), () => { throw Error("unnecessary request"); });
    assert.deepEqual(value, {state:"ready", steamid:id, game_count:1, games:[{appid:"10", name:"Owned"}]});
});
test("another profile and recent-only views cannot replace the owned library", async () => {
    const w = {g_steamID:id, g_rgProfileData:{steamid:"76561198000000001"}, rgGames:[{appid:10,name:"Not mine"}]};
    assert.equal((await run(...context({}, w))).state, "open-library");
    w.g_rgProfileData.steamid=id;
    assert.equal((await run(...context({}, w, "https://steamcommunity.com/my/games/?tab=recent"))).state, "open-library");
});
test("the web session stays in the Steam request and never the returned snapshot", async () => {
    const user = {logged_in:true, steamid:id, webapi_token:"test-session-only"};
    let requests = 0;
    const value = await run(...context(user), async (url, options) => {
        requests++;
        const parsed = new URL(url);
        assert.equal(parsed.origin, "https://api.steampowered.com");
        assert.equal(parsed.searchParams.get("access_token"), user.webapi_token);
        assert.equal(JSON.parse(parsed.searchParams.get("input_json")).steamid, id);
        assert.equal(options.redirect, "error");
        assert.equal(options.credentials, "omit");
        return {ok:true, text:async () => JSON.stringify({response:{game_count:1,games:[{appid:10,name:"Owned"}]}})};
    });
    assert.equal(requests, 1);
    assert.equal(value.state, "ready");
    assert.ok(!JSON.stringify(value).includes(user.webapi_token));
});
test("private, incomplete, duplicate and invalid responses do not become empty libraries", async () => {
    for (const response of [{}, {game_count:1,games:[]}, {game_count:2,games:[{appid:10,name:"A"},{appid:10,name:"A"}]},
                            {game_count:1,games:[{appid:"../10",name:"A"}]}, {game_count:1,games:[{appid:10,name:""}]}]) {
        const value = await run(...context({logged_in:true,steamid:id,webapi_token:"test"}), async () => ({ok:true,text:async()=>JSON.stringify({response})}));
        assert.equal(value.state, "unavailable");
    }
});
test("an explicitly empty owned library is valid", async () => {
    const value = await run(...context({logged_in:true,steamid:id,webapi_token:"test"}), async()=>({ok:true,text:async()=>JSON.stringify({response:{game_count:0}})}));
    assert.equal(value.state, "ready"); assert.deepEqual(value.games, []);
});
test("request failures return no raw URL, session or exception", async () => {
    const value = await run(...context({logged_in:true,steamid:id,webapi_token:"test-secret"}), async()=>{throw Error("test-secret");});
    assert.deepEqual(value, {state:"unavailable"});
});

test("conflicting account identity cannot import a stale profile", async () => {
    const value = await run(...context({logged_in:true, steamid:"76561198000000001", webapi_token:"test"}, {g_steamID:id}),
                            () => { throw Error("must not fetch"); });
    assert.deepEqual(value, {state:"unavailable"});
});

test("legacy signed-in page without application_config imports via Steam's own session", async () => {
    const ctx = context();
    ctx[1] = {getElementById:()=>null, querySelector:selector=>selector==="#account_pulldown" ? {} : null};
    const requests = [];
    const result = await run(...ctx, async (url, options) => {
        requests.push(url);
        if (url === "https://steamcommunity.com/chat/clientjstoken") {
            assert.equal(options.credentials, "same-origin");
            return {ok:true,text:async()=>JSON.stringify({logged_in:true,steamid:id,token:"session-only"})};
        }
        assert.equal(new URL(url).origin, "https://api.steampowered.com");
        assert.equal(options.credentials, "omit");
        return {ok:true,text:async()=>JSON.stringify({response:{game_count:1,games:[{appid:10,name:"Owned"}]}})};
    });
    assert.equal(requests.length, 2);
    assert.equal(result.state, "ready");
    assert.equal(result.games.length, 1);
    assert.ok(!JSON.stringify(result).includes("session-only"));
});
test("legacy session identity mismatch never fetches another account's games", async () => {
    let requests = 0;
    const result = await run(...context({}, {g_steamID:id}), async () => {
        requests++;
        return {ok:true,text:async()=>JSON.stringify({logged_in:true,steamid:"76561198000000001",token:"other"})};
    });
    assert.equal(result.state,"unavailable");
    assert.equal(requests,1);
});
test("legacy expired session requests sign-in and preserves old import", async () => {
    const ctx=context();
    ctx[1].querySelector=()=>({});
    const result=await run(...ctx, async()=>({ok:true,text:async()=>JSON.stringify({logged_in:false})}));
    assert.deepEqual(result,{state:"sign-in"});
});
test("partial legacy page can retry after its data finishes loading", async () => {
    const ctx=context({}, {g_steamID:id});
    const failed=await run(...ctx, async()=>{throw Error("offline");});
    assert.equal(failed.state,"unavailable");
    ctx[0].g_rgProfileData={steamid:id};
    ctx[0].rgGames=[{appid:10,name:"Owned"}];
    const retried=await run(...ctx, ()=>{throw Error("not needed");});
    assert.equal(retried.state,"ready");
});
