# Contributing to Soju

Three ways to help, smallest first.

## 1. Report a game

The most useful thing is a result from your Mac, whether the game ran or not.
[Open a compatibility report](https://github.com/BCD1210/soju/issues/new?template=compatibility.yml)
or post in `#compatibility` on [Discord](https://discord.gg/ttBDcF5ftz). Include:

- game and store (Steam, Battle.net, Epic, GOG, Xbox PC)
- Mac model and chip, macOS version
- Soju version (`soju --version`, or the number at the bottom left of the app)
- what happened, and the output of `soju doctor` if it failed

Remove account names and personal paths from logs before posting.

## 2. Docs and translations

- `README.md`, `README.ko.md`, `README.zh.md`
- Guides are hand-written HTML in `docs/guides/`
- The home pages are generated: edit `site/content/*.json`, then
  `cd site && npm install && npm run build`, and commit both the JSON and the
  files it writes under `docs/`

A guide for a game should say what was checked and what was not, on which Mac and macOS.

## 3. Code

```bash
git clone https://github.com/BCD1210/soju && cd soju
bash scripts/soju help            # the CLI runs from the checkout
bash scripts/install-app.sh       # builds the app and opens it (needs Apple's Command Line Tools)
```

Before opening a pull request, run what CI runs:

```bash
bash -n install.sh; for s in scripts/*.sh scripts/soju; do bash -n "$s"; done
python3 -m unittest discover -s tests -q
node --test tests/test_steam_login.cjs
swiftc app/PurchaseLinks.swift tests/test_purchase_links.swift -o /tmp/t1 && /tmp/t1
swiftc app/UpdateCheck.swift tests/test_update_check.swift -o /tmp/t2 && /tmp/t2
bash scripts/build-app.sh
```

Pull requests:

- One change per pull request. Say which Mac and macOS you ran it on.
- A new or changed behaviour comes with a test where the code is testable (see `tests/`).
- Commits and pull request text carry no AI attribution lines; the `commits` check fails on them.
- Do not add Apple Game Porting Toolkit binaries, CrossOver binaries, store clients or game files.
- Soju is GPL-3.0-or-later, and contributions are accepted under the same licence.

Not sure where to start? Issues labelled
[good first issue](https://github.com/BCD1210/soju/labels/good%20first%20issue) are small and self-contained.

## Security

Please do not open a public issue for a security problem. See [SECURITY.md](SECURITY.md).
