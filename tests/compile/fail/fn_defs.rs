use dyntable::*;

fn main() {}

#[dyntable(relax_abi = true)]
trait TypeGenerics {
	// type generics cannot be specified for fns
	fn foo<T>(&self, t: T);
}

#[dyntable(relax_abi = true)]
trait WhereClause {
	// where clauses cannot be specified for fns
	fn foo<'a>(&'a self)
	where
		'a: 'a;
}
