use openopen_skill_lifecycle::{
    ApprovalError, AuditAnchor, AuditError, GitHubAcquirer, GitHubRequest, LifecycleError,
    PromotionDecision, SkillLifecycle, SourceError,
};

const OLD_COMMIT: &str = "33abdecc92a4b8ef164a5a7a2b7b1ad13aadde07";
const NEW_COMMIT: &str = "fe5608d2512a7d6a7b9821ce8a88c48464ecd6e4";
const PACKAGE_PATH: &str = "plugins/sales/skills/prepare-for-meeting";

#[test]
fn canonical_github_url_surface_is_exact() {
    let request = GitHubRequest::parse(&format!(
        "https://github.com/openai/role-specific-plugins/tree/{OLD_COMMIT}/{PACKAGE_PATH}"
    ))
    .expect("canonical URL");
    assert_eq!(request.owner(), "openai");
    assert_eq!(request.repo(), "role-specific-plugins");
    assert_eq!(request.requested_ref(), Some(OLD_COMMIT));
    assert_eq!(request.package_path(), PACKAGE_PATH);
}

#[test]
fn noncanonical_github_urls_fail_closed() {
    for value in [
        "http://github.com/openai/repo",
        "https://user@github.com/openai/repo",
        "https://github.com:443/openai/repo",
        "https://127.0.0.1/openai/repo",
        "https://githuɓ.com/openai/repo",
        "https://github.com/openai/repo.git",
        "https://github.com/openai/repo?x=1",
        "https://github.com/openai/repo#fragment",
        "https://github.com/openai/repo/",
        "https://github.com/openai/repo/blob/main/SKILL.md",
        "https://github.com/openai/repo/tree/main/%2e%2e/other",
        "https://github.com/openai/repo/tree/main/../other",
        "https://github.com/openai/repo/tree/main/./other",
        "https://github.com/openai/repo/tree/main/a//b",
    ] {
        assert!(GitHubRequest::parse(value).is_err(), "accepted {value}");
    }
}

#[test]
fn approval_inputs_reject_controls_and_noncanonical_hashes() {
    assert_eq!(
        AuditAnchor::parse("A".repeat(64)),
        Err(ApprovalError::InvalidAuditAnchor)
    );
    assert_eq!(
        PromotionDecision::new("owner", "decision\u{202e}", "1".repeat(64)),
        Err(ApprovalError::InvalidDecisionIdentity)
    );
}

/// This is intentionally excluded from the deterministic matrix. Run it only
/// as explicit read-only public GitHub evidence. Both official pins must
/// remain Candidate because their exact immutable packages reference files
/// outside the selected package path.
#[test]
#[ignore = "explicit read-only GitHub evidence"]
fn live_preferred_openai_pins_are_negative_out_of_path_evidence() {
    let acquirer = GitHubAcquirer::default();
    for commit in [OLD_COMMIT, NEW_COMMIT] {
        let request = GitHubRequest::parse(&format!(
            "https://github.com/openai/role-specific-plugins/tree/{commit}/{PACKAGE_PATH}"
        ))
        .expect("canonical preferred URL");
        let package = acquirer.acquire(&request).expect("verified public package");
        assert_eq!(package.source().commit(), commit);
        let mut lifecycle = SkillLifecycle::from_candidate(package);
        let id = lifecycle.version_ids().next().expect("candidate");
        let revision = lifecycle.revision();
        assert_eq!(
            lifecycle.stage(
                revision,
                id,
                AuditAnchor::parse("a".repeat(64)).expect("audit anchor"),
            ),
            Err(LifecycleError::Audit(
                AuditError::OutOfPathOrMissingDependency
            ))
        );
        assert_eq!(lifecycle.revision(), revision);
    }
}

#[test]
fn immutable_requested_ref_mismatch_is_typed_at_source_binding() {
    let request =
        GitHubRequest::parse(&format!("https://github.com/openai/repo/tree/{OLD_COMMIT}"))
            .expect("canonical root URL");
    assert_eq!(
        openopen_skill_lifecycle::SkillSource::resolve(request, NEW_COMMIT),
        Err(SourceError::ResolvedCommitMismatch)
    );
}
