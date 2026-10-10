require "uri"
require "pathname"
require "securerandom"

if ENV["PARCELA_URL"]
  raise "Parcela preview configuration is development-only" unless Rails.env.development? && ENV["SHIRABE_PREVIEW"] == "1"
  raise "Do not chain rb-portless and Parcela" if ENV["PORTLESS_URL"]

  url = URI.parse(ENV.fetch("PARCELA_URL"))
  raise "Invalid Parcela public URL" unless url.is_a?(URI::HTTPS) && url.host == ENV.fetch("PARCELA_HOSTNAME") &&
    url.userinfo.nil? && [ "", "/" ].include?(url.path) && url.query.nil? && url.fragment.nil?

  runtime = Pathname.new(ENV.fetch("PARCELA_RUNTIME_DIR")).realpath
  stat = runtime.stat
  raise "Runtime must be private and user-owned" unless stat.directory? && stat.uid == Process.uid && (stat.mode & 0o077).zero?

  secret_path = runtime.join("rails-secret")
  secret_lock = File.open(runtime.join("rails-secret.lock"), File::RDWR | File::CREAT | File::NOFOLLOW, 0o600)
  begin
    secret_lock.flock(File::LOCK_EX)
    unless secret_path.exist?
      File.open(secret_path, File::WRONLY | File::CREAT | File::EXCL | File::NOFOLLOW, 0o600) do |file|
        file.write(SecureRandom.hex(64))
        file.fsync
      end
    end
    secret = File.open(secret_path, File::RDONLY | File::NOFOLLOW) do |file|
      raise "Invalid preview secret ownership" unless file.stat.uid == Process.uid && (file.stat.mode & 0o077).zero?
      file.read
    end
    raise "Invalid preview secret" unless secret.match?(/\A[0-9a-f]{128}\z/)
  ensure
    secret_lock.close
  end

  options = { host: url.host, protocol: "https" }
  options[:port] = url.port unless url.port == 443
  merge_url = ->(existing) do
    merged = existing.to_h.merge(options)
    merged.delete(:port) unless options.key?(:port)
    merged
  end

  Rails.application.configure do
    config.secret_key_base = secret
    config.log_level = :warn
    config.hosts = [ url.host ]
    config.assume_ssl = true
    config.paths["log"] = runtime.join("log/development.log").to_s
    config.action_mailer.delivery_method = :test
    config.action_mailer.perform_deliveries = true
    config.action_mailer.default_url_options = merge_url.call(config.action_mailer.default_url_options || {})
    config.action_cable.allowed_request_origins = [ "https://#{url.host}#{url.port == 443 ? "" : ":#{url.port}"}" ] if config.respond_to?(:action_cable)
    config.active_job.queue_adapter = ENV["SHIRABE_RUN_JOBS"] == "1" ? :solid_queue : :test
    config.session_store :cookie_store, key: "_shirabe_preview_session", domain: nil, secure: true, same_site: :lax
  end
  Rails.application.config.after_initialize do |app|
    app.routes.default_url_options.replace(merge_url.call(app.routes.default_url_options))
  end
end
