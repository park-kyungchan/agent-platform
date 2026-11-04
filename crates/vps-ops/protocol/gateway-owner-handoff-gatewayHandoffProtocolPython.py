
def run_handoff(driver):
 driver.revalidate_owner()
 driver.assert_quiescent()
 driver.park_intended_nonowner()
 driver.revalidate_owner()
 driver.assert_quiescent()
 driver.stop_owner_and_wait()
 driver.assert_both_down_and_lock_free()
 driver.start_intended()
 return driver.verify_intended_owner()
