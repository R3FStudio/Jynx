# Git workflow

## Ordinary Git

```sh
git status
git add .
git commit -m "Describe the change"
git push
```

The root repository contains public source. Local data, generated output, dependencies, and independently versioned checkouts are ignored. A commit guard rejects protected paths that were force-staged. A push guard checks the destination and refuses history containing protected paths.

## Linked checkouts

Install local hooks after cloning:

```sh
npm run repo:setup
```

`npm run repo:setup -- --clone` fetches configured checkouts that are not installed. Restricted repositories require appropriate authentication. Missing optional checkouts do not block normal public development.

Coordinate installed repositories from the root:

```sh
npm run repo:status
npm run repo:commit -- "Describe the change"
npm run repo:push
```

An unchanged repository gets no unnecessary commit. `repo:push` validates destinations and requires clean working trees before pushing. Combine the steps with:

```sh
npm run repo:sync -- "Describe the change"
```

Separate remotes cannot form one atomic transaction. If a later push fails, the command reports which repositories were pushed already. Fix the connection or permissions, then run `repo:push` again. Local commits remain intact.

Review the status and diff first. Use a message suitable for public changes. Hook installation is local to a clone; it is not a replacement for correct visibility and access settings on the hosting service.

Public source may be pushed to a public or private destination. Restricted linked source requires a verified private destination. The coordinator does not change repository visibility on GitHub.

## Distinct commit messages

The coordinator accepts a default message and per-repository overrides:

```sh
npm run repo:commit -- "Shared change summary" --message public "Improve harness request handling"
npm run repo:push
```

Use `--message ID "Subject"` for any installed scope listed in `repositories.config.json`. If no default is supplied, provide a message for every installed scope. Unknown scopes or missing messages fail before the first commit. Each checkout still receives its own commit and history. The `repo:sync` command accepts the same arguments.

Before a coordinated commit, inspect tracked diffs and new files in their owning checkout. Generated output and local machine settings should remain ignored. A successful commit is local until the push succeeds. A successful push is source publication; it is not a package release or package-registry upload.

## Branches and teammate changes

The coordinator pushes each checkout's current branch. It does not rename branches, create pull requests, or force a remote branch to match local history. For team work, use feature branches and ordinary review practices in each affected repository. Record required companion commit IDs in the review when a change crosses repository boundaries.

For a non-fast-forward rejection, fetch and review incoming commits in that checkout. Merge or rebase only after understanding them, rerun checks for conflicts, and retry the push. Do not delete commits or use force-push to bypass someone else's work.
