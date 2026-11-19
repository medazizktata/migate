# The base version's test suite inside a process that booted BEFORE the migration, the way a live server is during a deploy.
#   cd <base checkout> && ruby preboot_test.rb DIR
# Boots the app and warms it like a server that has served traffic (schema cache, prepared statements), then talks to the
# harness through flag files in DIR:
#   writes ready        -> harness migrates to head, writes go
#   runs the base suite in this same process, writes tests_done (and tests_rc)
#   waits for seeded    -> harness ran head's seed; this process reads those rows back through its cached models
#   writes readback_rc and exits with the suite's status
MV_PREBOOT = true
dir = ARGV.shift
flag = ->(n) { File.join(dir, n) }
wait = ->(n) { sleep 0.02 until File.exist?(flag.(n)) }
$LOAD_PATH.unshift File.expand_path("test")
require "test_helper"
require_relative "readback_rails"

Rails.application.eager_load!
ApplicationRecord.descendants.each do |m|
  m.columns # the schema cache a live process holds
  m.first   # and a prepared statement for its common query
end
Order.with_archive if Order.respond_to?(:with_archive)
File.write(flag.("ready"), Process.pid.to_s)
wait.("go")

Dir["test/**/*_test.rb"].sort.each { |f| require File.expand_path(f) }
ok = Minitest.run([])
$stdout.flush
File.write(flag.("tests_rc"), ok ? "0" : "1")
File.write(flag.("tests_done"), "")
wait.("seeded")
File.write(flag.("readback_rc"), mv_readback.to_s)
$stdout.flush
exit!(ok ? 0 : 1) # exit! skips minitest's at_exit autorun, which would run the suite a second time
