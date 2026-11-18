# Contributing to Kanso

Thank you for wanting to help. Kanso is a young project kept by very few people: small, clear
changes are the ones that get in fastest.

## Ways to help

- **Try it and tell us.** Kanso is tested by its automated tests and on very few PCs. A report
  that says "the EA app games show up, the Xbox ones do not" is worth a lot. The list of what
  nobody has seen working yet is in [docs/MANUAL-TESTS.md](docs/MANUAL-TESTS.md).
- **Make a theme or a plugin.** Neither needs a change to Kanso, and neither needs our
  approval: see [docs/THEMES.md](docs/THEMES.md) and [docs/PLUGINS.md](docs/PLUGINS.md). They
  are your own work, under the licence you choose.
- **Fix something or add something.** Open an issue first for anything larger than a fix, so
  that nobody builds what will not be merged.

## Before a pull request

How to build and what to run is in [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md). In short:

```bash
npm run build
```

```bash
npm run test:rust
```

And a few things a test cannot always catch:

- New text of the interface goes in **all four** dictionaries in `src/i18n/` (Italian,
  English, Spanish, Russian). Say so in the pull request if a translation is a best guess.
- New behaviour comes with a test where one is possible, and with a line in the guide that
  describes it (`docs/`). The guides are part of the product.
- Say plainly what you could not try on a real Windows PC. Nobody minds; everybody minds
  finding out later.
- No secrets, no personal data: no API keys, no account names, no paths with a user's name,
  in code, tests, fixtures or screenshots.
- The rules listed under *Rules the code keeps* in the developer guide are not up for a quiet
  change: themes stay data, plugins stay guests, nothing is sent to a server of ours.

## Licence

Kanso is free software under the [GNU General Public License, version 3 or later](LICENSE).
By contributing you agree that your contribution is released under the same licence.
