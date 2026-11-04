# Resident context resume

Repeated SVCs from the same guest no longer restore unchanged registers and trigger
full context-switch maintenance. Each saved context has a revision; the worker
skips restoration only when both thread ID and revision match its resident context.
A thread returning after running on another core therefore reloads its newer state.

`thread-final-100.txt`: 100/100 guest runs, 567 checks per run after this change.
The first corpus measurement before this optimization was 41 frames/17 stops/1 exit;
es2gears exceeded 15 seconds. The isolated follow-up produced a frame in 7.46 seconds.
Final corpus results are recorded separately, with the same default timeout.
