# First directed integration test failure

`search-tab-feature-tests.log` preserves 535UI PASS /1FAIL. The new test incorrectly equated empty History with retained_bytes()==0; the history contract charges a fixed4096B reserve even with no paths. The assertion now checks can_back/can_forward, not physical storage.

Source review also found the new snapshot charge initially omitted that4096B history reserve; it is now included before joint tab/pane admission, consistent with ordinary tab snapshots. Final tests cover closed-tab and other-pane budget rejection, without partial insertion. No resource or memory-saving claim follows from local accounting.
