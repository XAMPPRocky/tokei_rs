# tokei.rs badge service

tokei.rs has support for badges. For example
[![total lines](https://tokei.rs/b1/github/XAMPPRocky/tokei_rs)](https://github.com/XAMPPRocky/tokei_rs).

```sh
[![](https://tokei.rs/b1/github/XAMPPRocky/tokei_rs)](https://github.com/XAMPPRocky/tokei_rs).
```

Tokei's URL scheme is as follows.

```sh
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
[![lines of code](https://tokei.rs/b1/github/XAMPPRocky/tokei?category=code)](https://github.com/XAMPPRocky/tokei).

```sh
[![](https://tokei.rs/b1/github/XAMPPRocky/tokei?category=code)](https://github.com/XAMPPRocky/tokei).
```

You can customize the badge label by using the `?label=` query string. For example, [![custom label](https://tokei.rs/b1/github/XAMPPRocky/tokei?category=code&label=custom%20label)](https://github.com/XAMPPRocky/tokei).

```sh
[![](https://tokei.rs/b1/github/XAMPPRocky/tokei?category=code&label=custom%20label)](https://github.com/XAMPPRocky/tokei).
```

By default, the badge uses the "flat" style. You can customize the badge style by using the `?style=` query string. For example, [![lines of code](https://tokei.rs/b1/github/XAMPPRocky/tokei?category=code&style=for-the-badge)](https://github.com/XAMPPRocky/tokei).

```sh
[![](https://tokei.rs/b1/github/XAMPPRocky/tokei?category=code&style=for-the-badge)](https://github.com/XAMPPRocky/tokei).
```

## Copyright and License

(C) Copyright 2018 by XAMPPRocky and contributors

See [the graph](https://github.com/XAMPPRocky/tokei_rs/graphs/contributors) for a full list of contributors.

tokei.rs is distributed under the terms of both the MIT license and the Apache License (Version 2.0).

See [LICENSE-APACHE](./LICENSE-APACHE), [LICENSE-MIT](./LICENSE-MIT) for more information.
