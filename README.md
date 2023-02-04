Unfortunately this branch depends on undefined behavior, and its not possible to get around that.

Issues:
1. making a reference to an invalid fat pointer is always ub
2. moving fat pointers dropping bits
