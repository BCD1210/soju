// Ask the Battle.net Agent to install the Battle.net app, the way Blizzard's
// setup does. create-bottle.sh runs this under Wine's cscript when the setup
// stalls at 45% (issue #60).
// Usage: cscript //nologo bnet-agent-install.js <agent port> <install dir>
//
// The agent only answers clients running inside the bottle (curl from macOS
// gets 401), hence JScript + WinHttp instead of curl.
var args = WScript.Arguments, port = args(0), dir = args(1).replace(/\\/g, "/"), token = "";

function call(method, path, body) {
  var r = new ActiveXObject("WinHttp.WinHttpRequest.5.1");
  r.Open(method, "http://127.0.0.1:" + port + path, false);
  r.SetRequestHeader("User-Agent", "phoenix-agent/1.0");
  if (token) r.SetRequestHeader("Authorization", token);
  r.Send(body || "");
  if (r.Status < 200 || r.Status > 299) throw new Error(method + " " + path + ": HTTP " + r.Status + " " + r.ResponseText);
  return r.ResponseText;
}

function main() {
  var auth = /"authorization"\s*:\s*"([^"]+)"/.exec(call("GET", "/agent"));
  if (!auth) throw new Error("the agent returned no authorization token");
  token = auth[1];
  call("POST", "/install", '{"instructions_patch_url":"http://us.patch.battle.net:1119/bna",' +
    '"instructions_product":"NGDP","monitor_pid":12345,' +
    '"priority":{"insert_at_head":true,"value":900},"uid":"battle.net"}');
  call("POST", "/install/battle.net", '{"account_country":"USA","finalized":true,"game_dir":"' + dir + '",' +
    '"geo_ip_country":"US","language":["enus"],"selected_asset_locale":"enus",' +
    '"selected_locale":"enus","shortcut":"all","tome_torrent":""}');
  for (var i = 0; i < 450; i++) {   // 15 minutes
    var state = call("GET", "/install/battle.net");
    if (/"download_complete"\s*:\s*true/.test(state)) return 0;
    var progress = /"progress"\s*:\s*([0-9.]+)/.exec(state);
    if (progress && parseFloat(progress[1]) >= 1) return 0;
    WScript.Sleep(2000);
  }
  throw new Error("the install did not finish in 15 minutes");
}

try {
  WScript.Quit(main());
} catch (e) {
  WScript.Echo("    Agent install: " + e.message);
  WScript.Quit(1);
}
