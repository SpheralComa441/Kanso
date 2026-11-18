# Security

## Reporting a problem

Please do **not** open a public issue for something that could put users at risk. Use
GitHub's private report instead: the **Security** tab of this repository, *Report a
vulnerability* (or <https://github.com/SpheralComa441/Kanso/security/advisories/new>).

Say what you found, how to reproduce it and which version of Kanso you used. You will get an
answer as soon as the report is read; Kanso is kept by very few people in their spare time,
so allow a few days. A fix is published with a new release, and you are credited in its notes
unless you prefer not to be.

Only the latest release is supported.

## What Kanso is built to protect

Kanso runs on your PC, with your Windows user's rights, and has no server of its own. The
things it is designed never to let happen are these; a way around any of them is a
vulnerability, and we want to hear about it.

- **Your passwords.** Signing in to a store is optional and always happens on the store's own
  page. Kanso does not read, save or send a password or a two-factor code. What stays on the
  PC is the session the store issues (a token encrypted for your Windows user, or the store's
  own cookies in a browser profile used by that sign-in window alone); it is used with that
  store alone and deleted by *Sign out*. Details, store by store:
  [docs/ACCOUNTS.md](docs/ACCOUNTS.md).
- **Your data.** No telemetry, no analytics, no account with us. Requests go from your PC
  straight to the services Kanso reads (the stores and their public catalogues, SteamGridDB
  with your own key, the deal and news sources you keep switched on, a weather service if you
  turn the weather on, NVIDIA's repository if you turn DLSS downloads on, the hosts a plugin
  declares), and nowhere else. The full list, request by request: [PRIVACY.md](PRIVACY.md).
- **Themes are data.** A theme file is validated value by value against closed lists: it
  cannot carry CSS, scripts, paths or addresses of remote resources. Importing a stranger's
  theme can make Kanso ugly, not unsafe. See [docs/THEMES.md](docs/THEMES.md).
- **Plugins are guests.** A plugin runs in a sandbox with no access to files, to the
  interface or to Kanso's settings, and reaches only the `https` hosts its manifest declares,
  which you see before switching it on. What it returns is data that Kanso draws. A game
  brought by a plugin's library can open only a web page or a game client from a closed list
  the plugin declared. See [docs/PLUGINS.md](docs/PLUGINS.md).
- **What is started.** A game is started by its id in the local database; the address or the
  program to run never comes from the interface, a theme or a web page.
- **Your games' files.** Kanso changes none of them, with one exception you have to ask for:
  updating a game's DLSS libraries, which keeps the original next to the new file and only
  accepts a library Windows confirms as signed by NVIDIA. See [docs/DLSS.md](docs/DLSS.md).

## What is outside

- A plugin you install is code you chose to run in that sandbox, with the network access it
  declares: it can send what Kanso shows it (your list of games, never files or keys) to
  those hosts. Install plugins from people you have reason to trust.
- Kanso cannot protect you from other software on your PC, or from a store's own client.
- The installer is not signed with a publisher certificate yet, so Windows SmartScreen warns
  on first run. Download it only from this repository's Releases page, and compare its
  SHA-256 sum with the one published there.
