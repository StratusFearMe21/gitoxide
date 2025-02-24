use bstr::{BString, ByteVec};
use git_credentials::{helper, protocol::Context, Program};
use git_testtools::fixture_path;

#[test]
fn get() {
    let mut outcome = git_credentials::helper::invoke(
        &mut script_helper("last-pass"),
        &helper::Action::get_for_url("https://github.com/byron/gitoxide"),
    )
    .unwrap()
    .expect("mock provides credentials");
    assert_eq!(
        outcome.consume_identity().expect("complete"),
        git_sec::identity::Account {
            username: "user".into(),
            password: "pass".into()
        }
    );
    assert_eq!(
        outcome.next.store().payload().unwrap(),
        "username=user\npassword=pass\nquit=1\n"
    );
}

#[test]
fn store_and_reject() {
    let ctx = Context {
        url: Some("https://github.com/byron/gitoxide".into()),
        ..Default::default()
    };
    let ctxbuf = || -> BString {
        let mut buf = Vec::<u8>::new();
        ctx.write_to(&mut buf).expect("cannot fail");
        buf.into()
    };
    for action in [helper::Action::Store(ctxbuf()), helper::Action::Erase(ctxbuf())] {
        let outcome = git_credentials::helper::invoke(&mut script_helper("last-pass"), &action).unwrap();
        assert!(
            outcome.is_none(),
            "store and erase have no outcome, they just shouln't fail"
        );
    }
}

mod program {
    use git_credentials::{helper, program::Kind, Program};

    use crate::helper::invoke::script_helper;

    #[test]
    fn builtin() {
        assert!(
            matches!(
                git_credentials::helper::invoke(
                    &mut Program::from_kind(Kind::Builtin).suppress_stderr(),
                    &helper::Action::get_for_url("/path/without/scheme/fails/with/error"),
                )
                .unwrap_err(),
                helper::Error::CredentialsHelperFailed { .. }
            ),
            "this failure indicates we could launch the helper, even though it wasn't happy which is fine. It doesn't like the URL"
        );
    }

    #[test]
    fn script() {
        assert_eq!(
            git_credentials::helper::invoke(
                &mut Program::from_custom_definition(
                    "!f() { test \"$1\" = get && echo \"password=pass\" && echo \"username=user\"; }; f"
                ),
                &helper::Action::get_for_url("/does/not/matter"),
            )
            .unwrap()
            .expect("present")
            .consume_identity()
            .expect("complete"),
            git_sec::identity::Account {
                username: "user".into(),
                password: "pass".into()
            }
        );
    }

    #[cfg(unix)] // needs executable bits to work
    #[test]
    fn path_to_helper_script() -> crate::Result {
        assert_eq!(
            git_credentials::helper::invoke(
                &mut Program::from_custom_definition(
                    git_path::into_bstr(git_path::realpath(git_testtools::fixture_path("custom-helper.sh"))?)
                        .into_owned()
                ),
                &helper::Action::get_for_url("/does/not/matter"),
            )?
            .expect("present")
            .consume_identity()
            .expect("complete"),
            git_sec::identity::Account {
                username: "user-script".into(),
                password: "pass-script".into()
            }
        );
        Ok(())
    }

    #[test]
    fn path_to_helper_as_script_to_workaround_executable_bits() -> crate::Result {
        assert_eq!(
            git_credentials::helper::invoke(
                &mut script_helper("custom-helper"),
                &helper::Action::get_for_url("/does/not/matter")
            )?
            .expect("present")
            .consume_identity()
            .expect("complete"),
            git_sec::identity::Account {
                username: "user-script".into(),
                password: "pass-script".into()
            }
        );
        Ok(())
    }
}

pub fn script_helper(name: &str) -> Program {
    let mut script = git_path::to_unix_separators_on_windows(git_path::into_bstr(
        git_path::realpath(fixture_path(format!("{}.sh", name))).unwrap(),
    ))
    .into_owned();
    script.insert_str(0, "sh ");
    Program::from_kind(git_credentials::program::Kind::ExternalShellScript(script))
}
