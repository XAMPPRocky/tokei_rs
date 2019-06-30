# tokei.rs badge service

tokei.rs has support for badges. For example
[![](https://tokei.rs/b1/github/XAMPPRocky/tokei_rs)](https://github.com/XAMPPRocky/tokei_rs).

```
[![](https://tokei.rs/b1/github/XAMPPRocky/tokei_rs)](https://github.com/XAMPPRocky/tokei_rs).
```

Tokei's URL scheme is as follows.

```
https://tokei.rs/<host>/<namespace>/<name>
```

- `host`: either `github` or `gitlab`.
- `namespace`: The namespace of the repo. eg. `rust-lang` or `XAMPPRocky`.
- `name`: the name of the repo eg. `rust` or `tokei`.

By default the badge will show the repo's LoC(_Lines of Code_), you can also
specify for it to show a different category, by using the `?category=` query
string. It can be either `code`, `blanks`, `files`, `lines`, or `comments`.
Here is an example showing total number of lines.
[![](https://tokei.rs/b1/github/XAMPPRocky/tokei?category=lines)](https://github.com/XAMPPRocky/tokei).

```
[![](https://tokei.rs/b1/github/XAMPPRocky/tokei?category=lines)](https://github.com/XAMPPRocky/tokei).
```

