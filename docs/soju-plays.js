// Adds the games Soju players play (https://soju-stats.mikey9220.workers.dev/api/stats/summary, from the app's anonymous
// play counts) to the "Games that run" table. Rows already in the table are skipped.
(function () {
  var body = document.querySelector('table.proof tbody');
  if (!body || !window.fetch) return;
  var ko = document.documentElement.lang === 'ko';
  var stores = { battlenet: 'Battle.net', steam: ko ? '스팀' : 'Steam', epic: ko ? '에픽' : 'Epic', gog: 'GOG', ea: 'EA app',
    rockstar: 'Rockstar', ubisoft: 'Ubisoft', xbox: 'Xbox PC', didim: 'Didim Labs' };
  function norm(s) { return (s || '').toLowerCase().replace(/[^a-z0-9가-힣]+/g, ''); }
  var have = {};
  Array.prototype.forEach.call(body.rows, function (tr) {
    if (tr.getAttribute('data-game')) have[tr.getAttribute('data-game')] = 1;
    if (tr.cells[0]) have[norm(tr.cells[0].textContent)] = 1;
  });
  function cell(tag, text, cls) { var e = document.createElement(tag); if (cls) e.className = cls; e.textContent = text; return e; }
  fetch('https://soju-stats.mikey9220.workers.dev/api/stats/summary').then(function (r) { return r.ok ? r.json() : {}; }).then(function (d) {
    (d.played || []).slice(0, 30).forEach(function (g) {
      var k = norm(g.title);
      if (!k || have[k] || have[g.game]) return;
      have[k] = have[g.game] = 1;
      var hours = Math.max(1, Math.round(g.minutes / 60));
      var tr = document.createElement('tr');
      tr.setAttribute('data-game', g.game);
      var th = cell('th', g.title); th.scope = 'row'; tr.appendChild(th);
      tr.appendChild(cell('td', stores[g.launcher] || g.launcher));
      var td = document.createElement('td'); td.appendChild(cell('span', ko ? '플레이 됨' : 'Plays', 'pill plays')); tr.appendChild(td);
      tr.appendChild(cell('td', ko ? '소주 사용자 ' + g.players + '명이 플레이 (모두 합쳐 ' + hours + '시간)'
        : g.players + ' Soju players played it, ' + hours + ' hours in all.'));
      body.appendChild(tr);
    });
  }).catch(function () {});
})();
