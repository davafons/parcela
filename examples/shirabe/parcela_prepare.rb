require "digest"
require "json"

raise "Explicit disposable preview recipe required" unless Rails.env.development? && ENV["SHIRABE_PREVIEW"] == "1"
raise "Only disposable snapshots may use the fixture recipe" unless Rails.root.join(".parcela-disposable.json").file?

runtime = Pathname.new(ENV.fetch("PARCELA_RUNTIME_DIR")).realpath
configs = ActiveRecord::Base.configurations.configs_for(env_name: "development")
raise "Unexpected development database set" unless configs.map(&:name).sort == %w[cache dictionary primary queue]
paths = configs.map { |config| Pathname.new(config.database).expand_path(Rails.root) }
raise "Database escaped declared private data directory" unless paths.all? { |path| path.dirname.realpath == runtime.join("data").realpath }

inputs = %w[schema dictionary_schema queue_schema cache_schema].to_h do |name|
  [name, Digest::SHA256.file(Rails.root.join("db/#{name}.rb")).hexdigest]
end
marker = runtime.join("preparation.json")
if marker.file?
  previous = JSON.parse(marker.read)
  raise "Preparation inputs changed; explicit fresh baseline required" unless previous.fetch("schema_sha256") == inputs && paths.all?(&:file?)
else
  raise "Partial/unknown databases exist; never retry destructive preparation blindly" if paths.any?(&:exist?)
  Rails.application.load_tasks
  Rake::Task["db:schema:load"].invoke
  User.create!(email: "parcela-fixture@example.invalid", name: "Parcela Fixture", admin: true)
  raise "Database preparation incomplete" unless paths.all?(&:file?)
  File.open(marker, File::WRONLY | File::CREAT | File::EXCL, 0o600) do |file|
    file.write(JSON.generate(schema_sha256: inputs, baseline: "fresh schema and synthetic fixture user", dictionary_corpus: false))
    file.fsync
  end
end
puts JSON.generate(prepared: true, dictionary_corpus: false, automatic_restore: false)
