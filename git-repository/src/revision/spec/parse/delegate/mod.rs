use std::collections::HashSet;

use git_hash::ObjectId;
use git_revision::spec::{
    parse,
    parse::delegate::{self},
};
use smallvec::SmallVec;

use super::{Delegate, Error, ObjectKindHint};
use crate::{
    ext::{ObjectIdExt, ReferenceExt},
    Repository,
};

type Replacements = SmallVec<[(ObjectId, ObjectId); 1]>;

impl<'repo> Delegate<'repo> {
    pub fn new(repo: &'repo Repository, opts: crate::revision::spec::parse::Options) -> Self {
        Delegate {
            refs: Default::default(),
            objs: Default::default(),
            ambiguous_objects: Default::default(),
            idx: 0,
            kind: None,
            err: Vec::new(),
            prefix: Default::default(),
            last_call_was_disambiguate_prefix: Default::default(),
            opts,
            repo,
        }
    }

    pub fn into_err(mut self) -> Error {
        let repo = self.repo;
        for err in self
            .ambiguous_objects
            .iter_mut()
            .zip(self.prefix)
            .filter_map(|(a, b)| a.take().filter(|candidates| candidates.len() > 1).zip(b))
            .map(|(candidates, prefix)| Error::ambiguous(candidates, prefix, repo))
            .rev()
        {
            self.err.insert(0, err);
        }
        Error::from_errors(self.err)
    }

    pub fn into_rev_spec(mut self) -> Result<crate::revision::Spec<'repo>, Error> {
        fn zero_or_one_objects_or_ambguity_err(
            mut candidates: [Option<HashSet<ObjectId>>; 2],
            prefix: [Option<git_hash::Prefix>; 2],
            mut errors: Vec<Error>,
            repo: &Repository,
        ) -> Result<[Option<ObjectId>; 2], Error> {
            let mut out = [None, None];
            for ((candidates, prefix), out) in candidates.iter_mut().zip(prefix).zip(out.iter_mut()) {
                let candidates = candidates.take();
                match candidates {
                    None => *out = None,
                    Some(candidates) => {
                        match candidates.len() {
                            0 => unreachable!(
                                "BUG: let's avoid still being around if no candidate matched the requirements"
                            ),
                            1 => {
                                *out = candidates.into_iter().next();
                            }
                            _ => {
                                errors.insert(
                                    0,
                                    Error::ambiguous(candidates, prefix.expect("set when obtaining candidates"), repo),
                                );
                                return Err(Error::from_errors(errors));
                            }
                        };
                    }
                };
            }
            Ok(out)
        }

        fn kind_to_spec(
            kind: Option<git_revision::spec::Kind>,
            [first, second]: [Option<ObjectId>; 2],
        ) -> git_revision::Spec {
            use git_revision::spec::Kind::*;
            match kind.unwrap_or_default() {
                IncludeReachable => git_revision::Spec::Include(first.expect("set by parser")),
                ExcludeReachable => git_revision::Spec::Exclude(first.expect("set by parser")),
                RangeBetween => git_revision::Spec::Range {
                    from: first.expect("set by parser"),
                    to: second.expect("set by parser"),
                },
                ReachableToMergeBase => git_revision::Spec::Merge {
                    theirs: first.expect("set by parser"),
                    ours: second.expect("set by parser"),
                },
                IncludeReachableFromParents => git_revision::Spec::IncludeOnlyParents(first.expect("set by parser")),
                ExcludeReachableFromParents => git_revision::Spec::ExcludeParents(first.expect("set by parser")),
            }
        }

        let range = zero_or_one_objects_or_ambguity_err(self.objs, self.prefix, self.err, self.repo)?;
        Ok(crate::revision::Spec {
            first_ref: self.refs[0].take(),
            second_ref: self.refs[1].take(),
            inner: kind_to_spec(self.kind, range),
            repo: self.repo,
        })
    }
}

impl<'repo> parse::Delegate for Delegate<'repo> {
    fn done(&mut self) {
        self.follow_refs_to_objects_if_needed();
        self.disambiguate_objects_by_fallback_hint(
            self.kind_implies_committish()
                .then(|| ObjectKindHint::Committish)
                .or(self.opts.object_kind_hint),
        );
    }
}

impl<'repo> delegate::Kind for Delegate<'repo> {
    fn kind(&mut self, kind: git_revision::spec::Kind) -> Option<()> {
        use git_revision::spec::Kind::*;
        self.kind = Some(kind);

        if self.kind_implies_committish() {
            self.disambiguate_objects_by_fallback_hint(ObjectKindHint::Committish.into());
        }
        if matches!(kind, RangeBetween | ReachableToMergeBase) {
            self.idx += 1;
        }

        Some(())
    }
}

impl<'repo> Delegate<'repo> {
    fn kind_implies_committish(&self) -> bool {
        self.kind.unwrap_or(git_revision::spec::Kind::IncludeReachable) != git_revision::spec::Kind::IncludeReachable
    }
    fn disambiguate_objects_by_fallback_hint(&mut self, hint: Option<ObjectKindHint>) {
        fn require_object_kind(repo: &Repository, obj: &git_hash::oid, kind: git_object::Kind) -> Result<(), Error> {
            let obj = repo.find_object(obj)?;
            if obj.kind == kind {
                Ok(())
            } else {
                Err(Error::ObjectKind {
                    actual: obj.kind,
                    expected: kind,
                    oid: obj.id.attach(repo).shorten_or_id(),
                })
            }
        }

        if self.last_call_was_disambiguate_prefix[self.idx] {
            self.unset_disambiguate_call();

            if let Some(objs) = self.objs[self.idx].as_mut() {
                let repo = self.repo;
                let errors: Vec<_> = match hint {
                    Some(kind_hint) => match kind_hint {
                        ObjectKindHint::Treeish | ObjectKindHint::Committish => {
                            let kind = match kind_hint {
                                ObjectKindHint::Treeish => git_object::Kind::Tree,
                                ObjectKindHint::Committish => git_object::Kind::Commit,
                                _ => unreachable!("BUG: we narrow possibilities above"),
                            };
                            objs.iter()
                                .filter_map(|obj| peel(repo, obj, kind).err().map(|err| (*obj, err)))
                                .collect()
                        }
                        ObjectKindHint::Tree | ObjectKindHint::Commit | ObjectKindHint::Blob => {
                            let kind = match kind_hint {
                                ObjectKindHint::Tree => git_object::Kind::Tree,
                                ObjectKindHint::Commit => git_object::Kind::Commit,
                                ObjectKindHint::Blob => git_object::Kind::Blob,
                                _ => unreachable!("BUG: we narrow possibilities above"),
                            };
                            objs.iter()
                                .filter_map(|obj| require_object_kind(repo, obj, kind).err().map(|err| (*obj, err)))
                                .collect()
                        }
                    },
                    None => return,
                };

                if errors.len() == objs.len() {
                    self.err.extend(errors.into_iter().map(|(_, err)| err));
                } else {
                    for (obj, err) in errors {
                        objs.remove(&obj);
                        self.err.push(err);
                    }
                }
            }
        }
    }
    fn follow_refs_to_objects_if_needed(&mut self) -> Option<()> {
        assert_eq!(self.refs.len(), self.objs.len());
        let repo = self.repo;
        for (r, obj) in self.refs.iter().zip(self.objs.iter_mut()) {
            if let (_ref_opt @ Some(ref_), obj_opt @ None) = (r, obj) {
                if let Some(id) = ref_.target.try_id().map(ToOwned::to_owned).or_else(|| {
                    ref_.clone()
                        .attach(repo)
                        .peel_to_id_in_place()
                        .ok()
                        .map(|id| id.detach())
                }) {
                    obj_opt.get_or_insert_with(HashSet::default).insert(id);
                };
            };
        }
        Some(())
    }

    fn unset_disambiguate_call(&mut self) {
        self.last_call_was_disambiguate_prefix[self.idx] = false;
    }
}

fn peel(repo: &Repository, obj: &git_hash::oid, kind: git_object::Kind) -> Result<ObjectId, Error> {
    let mut obj = repo.find_object(obj)?;
    obj = obj.peel_to_kind(kind)?;
    debug_assert_eq!(obj.kind, kind, "bug in Object::peel_to_kind() which didn't deliver");
    Ok(obj.id)
}

fn handle_errors_and_replacements(
    destination: &mut Vec<Error>,
    objs: &mut HashSet<ObjectId>,
    errors: Vec<(ObjectId, Error)>,
    replacements: &mut Replacements,
) -> Option<()> {
    if errors.len() == objs.len() {
        destination.extend(errors.into_iter().map(|(_, err)| err));
        None
    } else {
        for (obj, err) in errors {
            objs.remove(&obj);
            destination.push(err);
        }
        for (find, replace) in replacements {
            objs.remove(find);
            objs.insert(*replace);
        }
        Some(())
    }
}

mod navigate;
mod revision;
