class ParcelaProbeController < ActionController::Base
  include Authentication
  protect_from_forgery with: :exception

  def show
    render json: {
      scheme: request.scheme, host: request.host, authenticated: authenticated?,
      csrf: form_authenticity_token, environment_id: ENV.fetch("PARCELA_ENVIRONMENT_ID"),
      route_url: Rails.application.routes.url_helpers.root_url,
      mailer_url_options: ActionMailer::Base.default_url_options,
      cable_origins: Rails.application.config.action_cable.allowed_request_origins,
      dictionary_entries: Dictionary::Entry.count, sessions: Session.count
    }
  end

  def create
    start_session(User.find_by!(email: "parcela-fixture@example.invalid"))
    render json: { authenticated: authenticated?, environment_id: ENV.fetch("PARCELA_ENVIRONMENT_ID") }
  end
end
