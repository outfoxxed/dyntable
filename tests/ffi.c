#include <stdlib.h>
#include <stddef.h>

static struct DebugFlags {
	unsigned int cdealloc_calls;
	unsigned int cdrop_calls;
} debug_flags = {
	.cdealloc_calls = 0,
	.cdrop_calls = 0,
};

struct DebugFlags* get_debug_flags() {
	return &debug_flags;
}

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

struct CValue {
	int value;
};

void drop_c_value() {
	debug_flags.cdrop_calls += 1;
}

int c_value_get(void* value) {
	struct CValue* v = value;
	return v->value;
}

void c_value_set(void* value, int set) {
	struct CValue* v = value;
	v->value = set;
}

static struct BoundedTraitVTable c_value_vtable = {
	.drop = &drop_c_value,
	.layout = {
		.size = sizeof(struct CValue),
		.align = _Alignof(struct CValue),
	},
	.parent = {
		.drop = &drop_c_value,
		.layout = {
			.size = sizeof(struct CValue),
			.align = _Alignof(struct CValue),
		},
		.get = &c_value_get,
	},
	.set = &c_value_set,
};

struct DynPtr new_c_value() {
	struct CValue* allocation = malloc(sizeof(struct CValue));
	allocation->value = 0;

	struct DynPtr ptr = {
		.ptr = allocation,
		.vtable = &c_value_vtable,
	};

	return ptr;
}

void dealloc_c_value(void* value) {
	free(value);
	debug_flags.cdealloc_calls += 1;
}


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
