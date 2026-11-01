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
