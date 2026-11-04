# command-corrections

## Testing

To manually test against your local Warp instance, navigate to [`warp/Cargo.toml`](https://github.com/warpdotdev/warp/blob/master/Cargo.toml) and change the `command-corrections` dependency from:

`command-corrections = { git = "https://github.com/warpdotdev/command-corrections.git", rev = "<some git sha>" }`

to:

`command-corrections = { path = "../../command-corrections" }`

(You might need to change the path if the repos aren't in sibling directories on your machine.)

## Publishing

Once you've merged your PR here, you'll need to go to [`warp/Cargo.toml`](https://github.com/warpdotdev/warp/blob/master/Cargo.toml) and make a PR to update the SHA for the `command-corrections` dependency. `git checkout main && git rev-parse HEAD | pbcopy` will put the SHA on your clipboard.

## Guidance for cross-platform command corrections

If you are writing a correction that you expect to be run in both `sh` shells and Powershell, please keep the following in mind:

### When suggesting a modification to a user-typed command, favor using the user's typed command

For example, if a user in powershell types:

```pwsh
cd..
```

You should correct to:

```pwsh
cd ..
```

and not

```pwsh
Set-Location ..
```

### When suggesting a new command / chained command, favor using the shell's idiomatic command

For example, if a user in powershell attempts to `cat` (`Get-Content`) a directory

```pwsh
cat src
```

You should correct to the idiomatic command `Get-ChildItem`

```pwsh
Get-ChildItem src
```

and not an alias

```pwsh
gci src
dir src
ls src
```

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE.md) file for details.  Many of the corrections were adapted from the [thefuck](https://github.com/nvbn/thefuck) project, which is also licensed under the MIT License.

Warp requires contributors to sign a contributor license agreement (CLA) before their contributions can be merged. You can read and sign our CLA at https://cla.warp.dev.
