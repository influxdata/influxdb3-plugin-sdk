# multi_cross_file_defect/__init__.py
# process_writes: declared AND defined, but `async def` (rejected).
# process_scheduled_call: declared but NOT defined (missing).

async def process_writes(a, b, c):
    pass
