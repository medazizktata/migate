# Rollback-compatibility probe from the BASE checkout: read every row (written by head's seed) through the base models.
#   fresh process:      cd <base checkout> && bin/rails runner <this file>
#   pre-booted process: preboot_test.rb calls mv_readback after loading this file
# Flags a failing query, an invalid record, and a numeric attribute whose stored value is not a number (Rails casts
# "shipped" to 0 silently, so validity alone would not see it).
def mv_readback
  Rails.application.eager_load!
  bad = []
  rows = 0
  ApplicationRecord.descendants.each do |m|
    m.find_each do |r|
      rows += 1
      bad << "#{m.name} id=#{r.id}: invalid #{r.errors.full_messages}" unless r.valid?
      m.attribute_types.each do |name, type|
        next unless %i[integer decimal float].include?(type.type)
        raw = r.read_attribute_before_type_cast(name)
        if raw.is_a?(String) && raw !~ /\A-?\d+(\.\d+)?\z/
          bad << "#{m.name} id=#{r.id}: #{name} stored #{raw.inspect}, read as #{r[name].inspect}"
        end
      end
    end
  rescue => e
    bad << "#{m.name} (query): #{e.class}: #{e.message.lines.first.strip}"
  end
  puts "read #{rows} rows"
  bad.each { |b| puts "READBACK #{b}" }
  bad.any? ? 1 : (rows.zero? ? 2 : 0)
end

exit(mv_readback) unless defined?(MV_PREBOOT)
