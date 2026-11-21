# wayward

The domain of a native Windows status bar: modules report state, a bar renders it, and one module reports AI usage.

## Language

**Usage window**:
Token consumption measured from local assistant logs over a bounded interval, compared against a user-defined limit.
_Avoid_: quota, rate limit

**Quota**:
A vendor-reported allowance and reset time for an AI provider, obtained from that provider's official usage API.
_Avoid_: limit, usage window
