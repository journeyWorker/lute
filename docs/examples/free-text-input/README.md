# free-text-input

Free-text input through a typed bridge (#171). `::ask{slot="password"}` is a blocking
bridge call; the host answers with the typed string, which lands in
`scene.ask.password.answer`, and the scene matches on it. No new construct: the page
[Choices and hubs → Free-text input](https://lute.vivident.dev/language/choices-and-hubs/)
describes the language half and
[Runtime contract](https://lute.vivident.dev/tooling/runtime-contract/) the host half.

```sh
lute check --deny-warnings --project docs/examples/free-text-input
lute test --project docs/examples/free-text-input
```
