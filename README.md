# tokei.rs badge service

tokei.rs has support for badges. For example
[![](https://tokei.rs/b1/github/XAMPPRocky/tokei_rs)](https://github.com/XAMPPRocky/tokei_rs).

```
[![](https://tokei.rs/b1/github/XAMPPRocky/tokei_rs)](https://github.com/XAMPPRocky/tokei_rs).
```

Tokei's URL scheme is as follows.

```
https://tokei.rs/b1/<domain>[<.com>]?/<namespace>/<repository>
```

- `domain`:  The domain name of git host. If no TLD is provided `.com` is added.
  e.g. `tokei.rs/b1/github` == `tokei.rs/b1/github.com`.
- `namespace`: The namespace of the repo. eg. `rust-lang` or `XAMPPRocky`.
- `repository`: the name of the repo eg. `rust` or `tokei`.

By default the badge will show the repo's total lines, you can also
specify for it to show a different category, by using the `?category=` query
string. It can be either `code`, `blanks`, `files`, `lines`, or `comments`.
Here is an example showing total number of code.
[![](https://tokei.rs/b1/github/XAMPPRocky/tokei?category=code)](https://github.com/XAMPPRocky/tokei).

```
[![](https://tokei.rs/b1/github/XAMPPRocky/tokei?category=lines)](https://github.com/XAMPPRocky/tokei).
```

