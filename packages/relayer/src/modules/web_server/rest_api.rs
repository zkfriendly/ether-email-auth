use crate::*;
use anyhow::Result;
use axum::{body::Body, response::Response, Json};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
pub struct RequestStatusRequest {
    pub request_id: String,
}

#[derive(Serialize, Deserialize)]
pub enum RequestStatus {
    NotExist = 0,
    Pending = 1,
    Processed = 2,
}

#[derive(Serialize, Deserialize)]
pub struct RequestStatusResponse {
    pub request_id: String,
    pub status: RequestStatus,
    pub is_success: bool,
    pub email_nullifier: Option<String>,
    pub account_salt: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct AcceptanceRequest {
    pub wallet_eth_addr: String,
    pub guardian_email_addr: String,
    pub account_code: String,
    pub template_idx: u64,
    pub subject: String,
}

#[derive(Serialize, Deserialize)]
pub struct AcceptanceResponse {
    pub request_id: String,
    pub subject_params: Vec<TemplateValue>,
}

#[derive(Serialize, Deserialize)]
pub struct RecoveryRequest {
    pub wallet_eth_addr: String,
    pub guardian_email_addr: String,
    pub template_idx: u64,
    pub subject: String,
}

#[derive(Serialize, Deserialize)]
pub struct RecoveryResponse {
    pub request_id: String,
    pub subject_params: Vec<TemplateValue>,
}

// Create request status API
pub async fn request_status_api(payload: RequestStatusRequest) -> Result<RequestStatusResponse> {
    let row = DB.get_request(payload.request_id.clone()).await?;
    let status = if let Some(ref row) = row {
        if row.is_processed {
            RequestStatus::Processed
        } else {
            RequestStatus::Pending
        }
    } else {
        RequestStatus::NotExist
    };
    Ok(RequestStatusResponse {
        request_id: payload.request_id,
        status,
        is_success: row
            .as_ref()
            .map_or(false, |r| r.is_success.unwrap_or(false)),
        email_nullifier: row.clone().and_then(|r| r.email_nullifier),
        account_salt: row.clone().and_then(|r| r.account_salt),
    })
}

pub async fn handle_acceptance_request(
    payload: AcceptanceRequest,
    db: Arc<Database>,
    email_sender: EmailForwardSender,
    chain_client: Arc<ChainClient>,
    tx_event_consumer: UnboundedSender<EmailAuthEvent>,
) -> Response<Body> {
    if !chain_client
        .is_wallet_deployed(&payload.wallet_eth_addr)
        .await
    {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from("Wallet not deployed"))
            .unwrap();
    }

    let subject_template = chain_client
        .get_acceptance_subject_templates(&payload.wallet_eth_addr, payload.template_idx)
        .await
        .unwrap();

    let subject_params = extract_template_vals(&payload.subject, subject_template);

    if subject_params.is_err() {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from("Invalid subject"))
            .unwrap();
    }

    if let Ok(Some(creds)) = db.get_credentials(&payload.account_code).await {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from("Account code already used"))
            .unwrap();
    }

    // let mut request_id = Uuid::new_v4().to_string();
    let mut request_id = "ad".to_string();
    // while let Ok(Some(request)) = db.get_request(&request_id).await {
    //     request_id = Uuid::new_v4().to_string(); // Regenerate request_id if it already exists
    // }

    if db
        .is_guardian_set(&payload.wallet_eth_addr, &payload.guardian_email_addr)
        .await
    {
        db.insert_request(&Request {
            request_id: request_id.clone(),
            wallet_eth_addr: payload.wallet_eth_addr.clone(),
            guardian_email_addr: payload.guardian_email_addr.clone(),
            is_for_recovery: false,
            template_idx: payload.template_idx,
            is_processed: false,
            is_success: None,
            email_nullifier: None,
            account_salt: None,
        })
        .await;

        tx_event_consumer.send(EmailAuthEvent::GuardianAlreadyExists {
            wallet_eth_addr: payload.wallet_eth_addr.clone(),
            guardian_email_addr: payload.guardian_email_addr.clone(),
        });
    } else {
        db.insert_credentials(&Credentials {
            account_code: payload.account_code.clone(),
            wallet_eth_addr: payload.wallet_eth_addr.clone(),
            guardian_email_addr: payload.guardian_email_addr.clone(),
            is_set: false,
        })
        .await;

        db.insert_request(&Request {
            request_id: request_id.clone(),
            wallet_eth_addr: payload.wallet_eth_addr.clone(),
            guardian_email_addr: payload.guardian_email_addr.clone(),
            is_for_recovery: false,
            template_idx: payload.template_idx,
            is_processed: false,
            is_success: None,
            email_nullifier: None,
            account_salt: None,
        });

        tx_event_consumer.send(EmailAuthEvent::Acceptance {
            wallet_eth_addr: payload.wallet_eth_addr.clone(),
            guardian_email_addr: payload.guardian_email_addr.clone(),
            request_id: request_id.clone(),
        });
    }

    Response::builder()
        .status(StatusCode::OK)
        .body(Body::from(
            serde_json::to_string(&AcceptanceResponse {
                request_id,
                subject_params: subject_params.unwrap(),
            })
            .unwrap(),
        ))
        .unwrap()
}
