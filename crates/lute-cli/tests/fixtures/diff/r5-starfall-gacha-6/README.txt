TOOL-DEFECT (0.26.0, minor): a schema error folded once "at the schema line" (dsl 0.26.0 §2.7) is
headed by the first importing file in path order, here the component file, which is reported
`failed` although it has no error of its own:
  ./components/hello.component.lute:1:1: error [E-USES-PARSE] schema import … (+1 more caller)
      world.schema.yaml:2:3: error [E-STATE-DECL] …
Run: lute check-project .
Meanwhile scenes/a.lute, which imports the same broken schema, is reported `ok`.
