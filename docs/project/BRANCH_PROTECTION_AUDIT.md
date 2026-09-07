# Branch Protection Audit

This repository supports a solo-maintainer mode for `main`.

Solo-maintainer mode keeps CI/status checks as the practical merge gate while
omitting the required approving-review gate. GitHub does not allow a pull
request author to approve their own pull request, so requiring one approval
blocks a repository that has only one write-capable human maintainer.

The `branch-protection-audit` workflow is a conservative recovery guard:

- it runs daily and can be run manually;
- it checks for a non-owner direct collaborator with write, maintain, or admin
  access;
- when such a collaborator exists, it restores one required approving review on
  `main`;
- it never removes or loosens branch protection.

Required secret:

- `BRANCH_PROTECTION_TOKEN`

The token must be able to read repository collaborators and update branch
protection. A fine-scoped GitHub token or GitHub App installation token is
preferred over a broad personal token.

Until this secret is configured, scheduled workflow runs exit successfully with
a notice and do not inspect or modify branch protection.

Restored review settings:

```json
{
  "dismiss_stale_reviews": true,
  "require_code_owner_reviews": false,
  "require_last_push_approval": false,
  "required_approving_review_count": 1
}
```

Manual dry run:

```text
Actions -> branch-protection-audit -> Run workflow -> dry_run=true
```
