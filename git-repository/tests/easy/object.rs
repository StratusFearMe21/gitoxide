use git_repository::easy;

mod in_empty_bare {
    use git_repository::prelude::ObjectAccessExt;

    #[test]
    fn write_empty_tree() -> crate::Result {
        let tmp = tempfile::tempdir()?;
        let repo = git_repository::init_bare(&tmp)?.into_easy();
        let oid = repo.write_object(&git_repository::objs::Tree::empty().into())?;
        assert_eq!(
            oid,
            git_repository::hash::ObjectId::empty_tree(),
            "it produces a well-known empty tree id"
        );
        Ok(())
    }
}

mod commit {
    use git_repository as git;
    use git_repository::prelude::{ObjectAccessExt, ReferenceAccessExt};
    use git_testtools::hex_to_id;

    #[test]
    fn multi_line_commit_message_uses_first_line_in_ref_log() {
        let (repo, _keep) = crate::basic_rw_repo().unwrap();
        let parent = repo.find_reference("HEAD").unwrap().peel_to_oid_in_place().unwrap();
        let tree_id = parent.object().unwrap().commit_iter().tree_id();
        let author = git::actor::Signature::empty();
        let commit_id = repo
            .commit(
                "HEAD",
                "hello there \r\n\nthe body",
                author.clone(),
                author,
                tree_id,
                Some(parent),
            )
            .unwrap();
        assert_eq!(
            commit_id,
            hex_to_id("1ff7decccf76bfa15bfdb0b66bac0c9144b4b083"),
            "the commit id is stable"
        );
    }
}

#[test]
fn object_ref_size_in_memory() {
    assert_eq!(
        std::mem::size_of::<easy::ObjectRef<'_, git_repository::Easy>>(),
        56,
        "the size of this structure should not changed unexpectedly"
    )
}

#[test]
fn oid_size_in_memory() {
    assert_eq!(
        std::mem::size_of::<easy::Oid<'_, git_repository::Easy>>(),
        32,
        "the size of this structure should not changed unexpectedly"
    )
}
