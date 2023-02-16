#include <stddef.h>

struct Layout {
	size_t size;
	size_t align;
};

struct ParentTraitVTable {
	void (*drop)(void*);
	struct Layout layout;
	int (*get)(void*);
};

struct BoundedTraitVTable {
	void (*drop)(void*);
	struct Layout layout;
	struct ParentTraitVTable parent;
	void (*set)(void*, int);
};

struct DynPtr {
	void* ptr;
	void* vtable;
};

void increment_bounded(struct DynPtr ptr) {
	struct BoundedTraitVTable* vtable = ptr.vtable;

	int value = vtable->parent.get(ptr.ptr);
	value += 1;
	vtable->set(ptr.ptr, value);
}

int get_parent(struct DynPtr ptr) {
	struct ParentTraitVTable* vtable = ptr.vtable;

	return vtable->get(ptr.ptr);
}
