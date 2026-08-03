# Lute project (`minimal` template)

Scaffolded by `lute init --template minimal`. Every file already passes the
checker.

## Next steps

```sh
# Validate the whole project (recursively):
lute check-project .

# Check one document:
lute check scenes/opening.lute

# Preview a scene against the trace mock:
lute trace scenes/opening.lute --mock mocks/playthrough.yaml

# Report the scene graph / reachability:
lute scenario .

# Add more documents:
lute new scene <name>
lute new quest <name>
lute new schema <name>
```
