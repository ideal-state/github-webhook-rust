*Event \- Pull Request*

pull request [\#${pull_request.number}](${pull_request.url}) of [${repository.full_name}](${repository.html_url}) ${action} by [${pull_request.user.full_name}](${pull_request.user.html_url}) \([${repository.owner.full_name}](${repository.owner.html_url})\)

```${pull_request.base.ref}<-${pull_request.head.ref}

${pull_request.title}

${pull_request.body}

${pull_request.created_at}

${pull_request.head.sha}
\.\.\.
${pull_request.merge_base}

```
*__\+${pull_request.additions}__* *__\-${pull_request.deletions}__* *\|* *__[diff](${pull_request.diff_url})__* *\|* *__[patch](${pull_request.patch_url})__* *\|* *__${pull_request.changed_files}__* *__changed file\(s\)__*