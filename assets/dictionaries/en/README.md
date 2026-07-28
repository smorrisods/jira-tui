# Bundled English dictionary

`index.aff`/`index.dic` are the Hunspell-format "en" (combined US/GB/CA/AU
spelling) dictionary from [`wooorm/dictionaries`][wooorm], itself generated
from [SCOWL][scowl] via `wordlist.aspell.net`. Used by `src/spellcheck.rs`
to highlight misspelled words in the built-in Markdown editor — see `LICENSE`
for the dictionary/affix file's own licence terms (MIT AND BSD), which is
separate from and compatible with this repository's MIT licence.

To refresh: download `dictionaries/en/index.aff`, `index.dic`, and `license`
from the `main` branch of [`wooorm/dictionaries`][wooorm] and replace the
files here (renaming `license` to `LICENSE`).

[wooorm]: https://github.com/wooorm/dictionaries
[scowl]: http://wordlist.aspell.net/dicts/
