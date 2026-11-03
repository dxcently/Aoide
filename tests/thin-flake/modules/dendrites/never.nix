# Throws if it is ever imported. No host selects `never`, so mkFlake must never read this.
throw "never read"
