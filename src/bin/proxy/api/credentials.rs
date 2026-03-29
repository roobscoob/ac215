use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use log::info;
use serde::{Deserialize, Serialize};

use crate::AppState;

type JsonResponse = (StatusCode, Json<serde_json::Value>);

fn ok_json(value: serde_json::Value) -> JsonResponse {
    (StatusCode::OK, Json(value))
}

fn created_json(value: serde_json::Value) -> JsonResponse {
    (StatusCode::CREATED, Json(value))
}

fn error_json(status: StatusCode, message: &str) -> JsonResponse {
    (
        status,
        Json(serde_json::json!({ "ok": false, "error": message })),
    )
}

fn internal_error(e: impl std::fmt::Display) -> JsonResponse {
    error_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string())
}

// ─── DB helpers ───
// These fully consume the QueryStream before returning, so the caller can
// borrow the client again (e.g. for rollback) without a double-borrow conflict.

async fn rollback(db: &mut crate::db::DbClient) {
    if let Ok(stream) = db.simple_query("IF @@TRANCOUNT > 0 ROLLBACK").await {
        let _ = stream.into_results().await;
    }
}

async fn query_opt_row(
    db: &mut crate::db::DbClient,
    sql: &str,
    params: &[&dyn tiberius::ToSql],
) -> Result<Option<tiberius::Row>, String> {
    match db.query(sql, params).await {
        Ok(stream) => stream.into_row().await.map_err(|e| e.to_string()),
        Err(e) => Err(e.to_string()),
    }
}

async fn query_exists(
    db: &mut crate::db::DbClient,
    sql: &str,
    params: &[&dyn tiberius::ToSql],
) -> Result<bool, String> {
    query_opt_row(db, sql, params)
        .await
        .map(|opt| opt.is_some())
}

async fn query_rows(
    db: &mut crate::db::DbClient,
    sql: &str,
    params: &[&dyn tiberius::ToSql],
) -> Result<Vec<tiberius::Row>, String> {
    match db.query(sql, params).await {
        Ok(stream) => stream.into_first_result().await.map_err(|e| e.to_string()),
        Err(e) => Err(e.to_string()),
    }
}

/// Begin a transaction. Returns an error response on failure.
async fn begin(db: &mut crate::db::DbClient) -> Option<JsonResponse> {
    // Clean up any leaked transaction from a cancelled request.
    rollback(db).await;
    match db.simple_query("BEGIN TRAN").await {
        Ok(stream) => { let _ = stream.into_results().await; None }
        Err(e) => Some(internal_error(e)),
    }
}

/// Commit a transaction, rolling back on failure. Returns an error response on failure.
async fn commit(db: &mut crate::db::DbClient) -> Option<JsonResponse> {
    let err = match db.simple_query("COMMIT").await {
        Ok(stream) => { let _ = stream.into_results().await; return None; }
        Err(e) => e.to_string(),
    };
    rollback(db).await;
    Some(internal_error(err))
}

// ─── User types ───

#[derive(Deserialize)]
pub struct CreateUserRequest {
    pub first_name: String,
    pub last_name: String,
    #[serde(default = "default_department")]
    pub department: i32,
    #[serde(default = "default_access_group")]
    pub access_group: i32,
    #[serde(default)]
    pub pin: String,
    #[serde(default)]
    pub master_user: bool,
    #[serde(default)]
    pub notes: String,
}

fn default_department() -> i32 {
    1
}
fn default_access_group() -> i32 {
    2
}

#[derive(Deserialize)]
pub struct UpdateUserRequest {
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub department: Option<i32>,
    pub access_group: Option<i32>,
    pub pin: Option<String>,
    pub master_user: Option<bool>,
    pub notes: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct UserResponse {
    pub id: i32,
    pub first_name: String,
    pub last_name: String,
    pub department: i32,
    pub access_group: i32,
    pub master_user: bool,
    pub notes: String,
}

// ─── Card types ───

#[derive(Deserialize)]
pub struct CreateCardRequest {
    pub site_code: i32,
    pub card_code: String,
}

#[derive(Deserialize)]
pub struct AssignCardRequest {
    pub employee_id: i32,
}

#[derive(Serialize, Clone)]
pub struct CardResponse {
    pub id: i32,
    pub site_code: i32,
    pub card_code: String,
    pub status: String,
    pub employee_id: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub employee: Option<UserResponse>,
}

// ─── Validation ───

const VALID_DEPARTMENTS: &[i32] = &[1, 2, 10000];
const VALID_ACCESS_GROUPS: &[i32] = &[1, 2, 1000000];

fn validate_department(id: i32) -> Option<JsonResponse> {
    if VALID_DEPARTMENTS.contains(&id) {
        None
    } else {
        Some(error_json(StatusCode::BAD_REQUEST, "invalid department"))
    }
}

fn validate_access_group(id: i32) -> Option<JsonResponse> {
    if VALID_ACCESS_GROUPS.contains(&id) {
        None
    } else {
        Some(error_json(StatusCode::BAD_REQUEST, "invalid access_group"))
    }
}

// ─── Row helpers ───

fn read_user(row: &tiberius::Row) -> UserResponse {
    UserResponse {
        id: row.get("iEmployeeNum").unwrap_or(0),
        first_name: row.get::<&str, _>("tFirstName").unwrap_or("").to_string(),
        last_name: row.get::<&str, _>("tLastName").unwrap_or("").to_string(),
        department: row.get("IdDepartment").unwrap_or(0),
        access_group: row.get("IdAccessGroup").unwrap_or(0),
        master_user: row.get::<bool, _>("bMasterUser").unwrap_or(false),
        notes: row.get::<&str, _>("tNotes").unwrap_or("").to_string(),
    }
}

fn read_card(row: &tiberius::Row) -> CardResponse {
    CardResponse {
        id: row.get("IdCardNum").unwrap_or(0),
        site_code: row.get("iSiteCode").unwrap_or(0),
        card_code: row.get::<&str, _>("iCardCode").unwrap_or("").to_string(),
        status: match row.get::<u8, _>("eCardStatus").unwrap_or(0) {
            1 => "active".to_string(),
            _ => "unassigned".to_string(),
        },
        employee_id: row.get("IdEmpNum").unwrap_or(0),
        employee: None,
    }
}

/// Read a card from a row produced by a LEFT JOIN with tblEmployees.
/// Employee columns are aliased as emp_*.
fn read_card_with_employee(row: &tiberius::Row) -> CardResponse {
    let emp_id: i32 = row.get("IdEmpNum").unwrap_or(0);
    let employee = if emp_id != 0 {
        Some(UserResponse {
            id: emp_id,
            first_name: row.get::<&str, _>("emp_first").unwrap_or("").to_string(),
            last_name: row.get::<&str, _>("emp_last").unwrap_or("").to_string(),
            department: row.get("emp_dept").unwrap_or(0),
            access_group: row.get("emp_ag").unwrap_or(0),
            master_user: row.get::<bool, _>("emp_master").unwrap_or(false),
            notes: row.get::<&str, _>("emp_notes").unwrap_or("").to_string(),
        })
    } else {
        None
    };

    CardResponse {
        id: row.get("IdCardNum").unwrap_or(0),
        site_code: row.get("iSiteCode").unwrap_or(0),
        card_code: row.get::<&str, _>("iCardCode").unwrap_or("").to_string(),
        status: match row.get::<u8, _>("eCardStatus").unwrap_or(0) {
            1 => "active".to_string(),
            _ => "unassigned".to_string(),
        },
        employee_id: emp_id,
        employee,
    }
}

const CARD_JOIN_SELECT: &str =
    "SELECT c.IdCardNum, c.iSiteCode, c.iCardCode, c.eCardStatus, c.IdEmpNum,
            e.tFirstName AS emp_first, e.tLastName AS emp_last,
            e.IdDepartment AS emp_dept, e.IdAccessGroup AS emp_ag, e.bMasterUser AS emp_master,
            e.tNotes AS emp_notes
     FROM tblCard c
     LEFT JOIN tblEmployees e ON c.IdEmpNum = e.iEmployeeNum";

// ─── User handlers ───

/// POST /v1/users
pub async fn create_user(
    State(state): State<AppState>,
    Json(req): Json<CreateUserRequest>,
) -> JsonResponse {
    if let Some(err) = validate_department(req.department) {
        return err;
    }
    if let Some(err) = validate_access_group(req.access_group) {
        return err;
    }

    let mut db = state.db.lock().await;

    if let Some(err) = begin(&mut db).await {
        return err;
    }

    let emp_id: i32 = match query_opt_row(
        &mut db,
        "SELECT ISNULL(MAX(iEmployeeNum), 0) + 1 FROM tblEmployees WITH (UPDLOCK, HOLDLOCK)",
        &[],
    )
    .await
    {
        Ok(Some(row)) => row.get(0).unwrap_or(1),
        Ok(None) => {
            rollback(&mut db).await;
            return internal_error("failed to allocate employee ID");
        }
        Err(e) => {
            rollback(&mut db).await;
            return internal_error(e);
        }
    };

    let result = db
        .execute(
            "INSERT INTO tblEmployees (
                iEmployeeNum, tFirstName, tLastName, IdDepartment,
                IdAccessGroup, dtStartDate, dtStopDate, bValidDate,
                dtEmpDate, tCodePIN, IdTimeGroup, IdOutputsGroup, iCounter,
                EmpNumCompany, bMasterUser, tNotes
            ) VALUES (
                @P1, @P2, @P3, @P4,
                @P5, GETDATE(), GETDATE(), 0,
                GETDATE(), @P6, 1, 0, 1,
                @P7, @P8, @P9
            )",
            &[
                &emp_id,
                &req.first_name.as_str(),
                &req.last_name.as_str(),
                &req.department,
                &req.access_group,
                &req.pin.as_str(),
                &emp_id,
                &req.master_user,
                &req.notes.as_str(),
            ],
        )
        .await;

    match result {
        Ok(_) => {
            if let Some(err) = commit(&mut db).await {
                return err;
            }
            info!(
                "created employee {emp_id}: {} {}",
                req.first_name, req.last_name
            );
            created_json(serde_json::json!({
                "ok": true,
                "user": {
                    "id": emp_id,
                    "first_name": req.first_name,
                    "last_name": req.last_name,
                    "department": req.department,
                    "access_group": req.access_group,
                    "master_user": req.master_user,
                    "notes": req.notes,
                }
            }))
        }
        Err(e) => {
            rollback(&mut db).await;
            internal_error(e)
        }
    }
}

/// GET /v1/users
pub async fn list_users(State(state): State<AppState>) -> JsonResponse {
    let mut db = state.db.lock().await;

    let rows = match query_rows(
        &mut db,
        "SELECT iEmployeeNum, tFirstName, tLastName, IdDepartment, IdAccessGroup, bMasterUser, tNotes
         FROM tblEmployees ORDER BY iEmployeeNum",
        &[],
    )
    .await
    {
        Ok(rows) => rows,
        Err(e) => return internal_error(e),
    };

    let users: Vec<UserResponse> = rows.iter().map(read_user).collect();
    ok_json(serde_json::json!({ "ok": true, "users": users }))
}

/// GET /v1/users/:id
pub async fn get_user(State(state): State<AppState>, Path(id): Path<i32>) -> JsonResponse {
    let mut db = state.db.lock().await;

    let row = match query_opt_row(
        &mut db,
        "SELECT iEmployeeNum, tFirstName, tLastName, IdDepartment, IdAccessGroup, bMasterUser, tNotes
         FROM tblEmployees WHERE iEmployeeNum = @P1",
        &[&id],
    )
    .await
    {
        Ok(Some(row)) => row,
        Ok(None) => return error_json(StatusCode::NOT_FOUND, "user not found"),
        Err(e) => return internal_error(e),
    };

    let user = read_user(&row);

    let card_rows = match query_rows(
        &mut db,
        "SELECT IdCardNum, iSiteCode, iCardCode, eCardStatus, IdEmpNum
         FROM tblCard WHERE IdEmpNum = @P1",
        &[&id],
    )
    .await
    {
        Ok(rows) => rows,
        Err(e) => return internal_error(e),
    };

    let cards: Vec<CardResponse> = card_rows.iter().map(read_card).collect();

    ok_json(serde_json::json!({
        "ok": true,
        "user": {
            "id": user.id,
            "first_name": user.first_name,
            "last_name": user.last_name,
            "department": user.department,
            "access_group": user.access_group,
            "master_user": user.master_user,
            "notes": user.notes,
            "cards": cards,
        }
    }))
}

/// PATCH /v1/users/:id
pub async fn update_user(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Json(req): Json<UpdateUserRequest>,
) -> JsonResponse {
    if let Some(dept) = req.department {
        if let Some(err) = validate_department(dept) {
            return err;
        }
    }
    if let Some(ag) = req.access_group {
        if let Some(err) = validate_access_group(ag) {
            return err;
        }
    }

    let mut db = state.db.lock().await;

    if let Some(err) = begin(&mut db).await {
        return err;
    }

    let exists = match query_exists(
        &mut db,
        "SELECT 1 FROM tblEmployees WHERE iEmployeeNum = @P1",
        &[&id],
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            rollback(&mut db).await;
            return internal_error(e);
        }
    };
    if !exists {
        rollback(&mut db).await;
        return error_json(StatusCode::NOT_FOUND, "user not found");
    }

    // Build dynamic SET clause. @P1 is always the employee ID.
    let mut param_idx = 1u8;

    struct SetClause {
        sql: String,
    }

    let mut clauses: Vec<SetClause> = Vec::new();

    if req.first_name.is_some() {
        param_idx += 1;
        clauses.push(SetClause {
            sql: format!("tFirstName = @P{param_idx}"),
        });
    }
    if req.last_name.is_some() {
        param_idx += 1;
        clauses.push(SetClause {
            sql: format!("tLastName = @P{param_idx}"),
        });
    }
    if req.department.is_some() {
        param_idx += 1;
        clauses.push(SetClause {
            sql: format!("IdDepartment = @P{param_idx}"),
        });
    }
    if req.access_group.is_some() {
        param_idx += 1;
        clauses.push(SetClause {
            sql: format!("IdAccessGroup = @P{param_idx}"),
        });
    }
    if req.pin.is_some() {
        param_idx += 1;
        clauses.push(SetClause {
            sql: format!("tCodePIN = @P{param_idx}"),
        });
    }
    if req.master_user.is_some() {
        param_idx += 1;
        clauses.push(SetClause {
            sql: format!("bMasterUser = @P{param_idx}"),
        });
    }
    if req.notes.is_some() {
        param_idx += 1;
        clauses.push(SetClause {
            sql: format!("tNotes = @P{param_idx}"),
        });
    }

    if clauses.is_empty() {
        rollback(&mut db).await;
        return error_json(StatusCode::BAD_REQUEST, "no fields to update");
    }

    let set_sql: Vec<&str> = clauses.iter().map(|c| c.sql.as_str()).collect();
    let query = format!(
        "UPDATE tblEmployees SET {} WHERE iEmployeeNum = @P1",
        set_sql.join(", ")
    );

    let mut query_params: Vec<&dyn tiberius::ToSql> = vec![&id];
    if let Some(ref v) = req.first_name {
        query_params.push(v as &dyn tiberius::ToSql);
    }
    if let Some(ref v) = req.last_name {
        query_params.push(v as &dyn tiberius::ToSql);
    }
    if let Some(ref v) = req.department {
        query_params.push(v as &dyn tiberius::ToSql);
    }
    if let Some(ref v) = req.access_group {
        query_params.push(v as &dyn tiberius::ToSql);
    }
    if let Some(ref v) = req.pin {
        query_params.push(v as &dyn tiberius::ToSql);
    }
    if let Some(ref v) = req.master_user {
        query_params.push(v as &dyn tiberius::ToSql);
    }
    if let Some(ref v) = req.notes {
        query_params.push(v as &dyn tiberius::ToSql);
    }

    if let Err(e) = db.execute(query.as_str(), &query_params).await {
        rollback(&mut db).await;
        return internal_error(e);
    }

    info!("updated employee {id}");

    // If access-related fields changed and user has a panel slot, re-push user data.
    if req.access_group.is_some() || req.pin.is_some() || req.master_user.is_some() {
        let slot_opt: Option<i32> = match query_opt_row(
            &mut db,
            "SELECT IdUserSlot FROM tblSlotUser WHERE IdUserNum = @P1 AND IdPanel = 1",
            &[&id],
        )
        .await
        {
            Ok(Some(row)) => row.get("IdUserSlot"),
            _ => None,
        };

        if let Some(user_slot) = slot_opt {
            if let Err(e) = db
                .execute(
                    "INSERT INTO tblDownload (dtDate, IdNetwork, IdPanel, iPriority, eCommand, IdRecord, iTemp1)
                     VALUES (GETDATE(), 1, 1, 140, 52, @P1, @P2)",
                    &[&id, &user_slot],
                )
                .await
            {
                rollback(&mut db).await;
                return internal_error(e);
            }
            info!("queued UserData download for employee {id} slot {user_slot}");
        }
    }

    if let Some(err) = commit(&mut db).await {
        return err;
    }
    ok_json(serde_json::json!({ "ok": true }))
}

/// DELETE /v1/users/:id
pub async fn delete_user(State(state): State<AppState>, Path(id): Path<i32>) -> JsonResponse {
    let mut db = state.db.lock().await;

    if let Some(err) = begin(&mut db).await {
        return err;
    }

    let exists = match query_exists(
        &mut db,
        "SELECT 1 FROM tblEmployees WHERE iEmployeeNum = @P1",
        &[&id],
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            rollback(&mut db).await;
            return internal_error(e);
        }
    };
    if !exists {
        rollback(&mut db).await;
        return error_json(StatusCode::NOT_FOUND, "user not found");
    }

    // Check for any cards referencing this employee (including legacy data with eCardStatus=0).
    let has_cards = match query_exists(
        &mut db,
        "SELECT 1 FROM tblCard WHERE IdEmpNum = @P1",
        &[&id],
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            rollback(&mut db).await;
            return internal_error(e);
        }
    };
    if has_cards {
        rollback(&mut db).await;
        return error_json(StatusCode::CONFLICT, "user still has assigned cards");
    }

    // Queue user deletion on panel if they have a slot.
    let user_slot: Option<i32> = match query_opt_row(
        &mut db,
        "SELECT IdUserSlot FROM tblSlotUser WHERE IdUserNum = @P1 AND IdPanel = 1",
        &[&id],
    )
    .await
    {
        Ok(Some(row)) => Some(row.get("IdUserSlot").unwrap_or(0)),
        _ => None,
    };

    if let Some(slot) = user_slot {
        if let Err(e) = db
            .execute(
                "INSERT INTO tblDownload (dtDate, IdNetwork, IdPanel, iPriority, eCommand, IdRecord, iTemp1)
                 VALUES (GETDATE(), 1, 1, 140, 241, @P1, @P2)",
                &[&id, &slot],
            )
            .await
        {
            rollback(&mut db).await;
            return internal_error(e);
        }
    }

    if let Err(e) = db
        .execute(
            "DELETE FROM tblSlotUser WHERE IdUserNum = @P1 AND IdPanel = 1",
            &[&id],
        )
        .await
    {
        rollback(&mut db).await;
        return internal_error(e);
    }

    if let Err(e) = db
        .execute("DELETE FROM tblEmployees WHERE iEmployeeNum = @P1", &[&id])
        .await
    {
        rollback(&mut db).await;
        return internal_error(e);
    }

    if let Some(err) = commit(&mut db).await {
        return err;
    }
    info!("deleted employee {id}");
    ok_json(serde_json::json!({ "ok": true }))
}

// ─── Card handlers ───

/// POST /v1/cards
pub async fn create_card(
    State(state): State<AppState>,
    Json(req): Json<CreateCardRequest>,
) -> JsonResponse {
    let mut db = state.db.lock().await;

    if let Some(err) = begin(&mut db).await {
        return err;
    }

    // Check for duplicate (site_code, card_code).
    let dup = match query_exists(
        &mut db,
        "SELECT 1 FROM tblCard WHERE iSiteCode = @P1 AND iCardCode = @P2",
        &[&req.site_code, &req.card_code.as_str()],
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            rollback(&mut db).await;
            return internal_error(e);
        }
    };
    if dup {
        rollback(&mut db).await;
        return error_json(
            StatusCode::CONFLICT,
            "card with this site_code and card_code already exists",
        );
    }

    let card_id: i32 = match query_opt_row(
        &mut db,
        "SELECT ISNULL(MAX(IdCardNum), 0) + 1 FROM tblCard WITH (UPDLOCK, HOLDLOCK)",
        &[],
    )
    .await
    {
        Ok(Some(row)) => row.get(0).unwrap_or(1),
        Ok(None) => {
            rollback(&mut db).await;
            return internal_error("failed to allocate card ID");
        }
        Err(e) => {
            rollback(&mut db).await;
            return internal_error(e);
        }
    };

    // Format description: "198, 000000000001093"
    let desc = format!("{}, {:0>15}", req.site_code, req.card_code);

    let result = db
        .execute(
            "INSERT INTO tblCard (
                IdCardNum, iSiteCode, iCardCode, tDescCard,
                eCardType, eCardStatus, IdEmpNum,
                iFacilityCodeSecond, iIssueNumber, CredentialType
            ) VALUES (
                @P1, @P2, @P3, @P4,
                1, 0, 0,
                -1, -1, 1
            )",
            &[
                &card_id,
                &req.site_code,
                &req.card_code.as_str(),
                &desc.as_str(),
            ],
        )
        .await;

    match result {
        Ok(_) => {
            if let Some(err) = commit(&mut db).await {
                return err;
            }
            info!(
                "created card {card_id}: site={} code={}",
                req.site_code, req.card_code
            );
            created_json(serde_json::json!({
                "ok": true,
                "card": {
                    "id": card_id,
                    "site_code": req.site_code,
                    "card_code": req.card_code,
                    "status": "unassigned",
                    "employee_id": 0,
                }
            }))
        }
        Err(e) => {
            rollback(&mut db).await;
            internal_error(e)
        }
    }
}

/// GET /v1/cards
pub async fn list_cards(State(state): State<AppState>) -> JsonResponse {
    let mut db = state.db.lock().await;

    let sql = format!("{CARD_JOIN_SELECT} ORDER BY c.IdCardNum");
    let rows = match query_rows(&mut db, &sql, &[]).await {
        Ok(rows) => rows,
        Err(e) => return internal_error(e),
    };

    let cards: Vec<CardResponse> = rows.iter().map(read_card_with_employee).collect();
    ok_json(serde_json::json!({ "ok": true, "cards": cards }))
}

/// GET /v1/cards/:id
pub async fn get_card(State(state): State<AppState>, Path(id): Path<i32>) -> JsonResponse {
    let mut db = state.db.lock().await;

    let sql = format!("{CARD_JOIN_SELECT} WHERE c.IdCardNum = @P1");
    let row = match query_opt_row(&mut db, &sql, &[&id]).await {
        Ok(Some(row)) => row,
        Ok(None) => return error_json(StatusCode::NOT_FOUND, "card not found"),
        Err(e) => return internal_error(e),
    };

    ok_json(serde_json::json!({ "ok": true, "card": read_card_with_employee(&row) }))
}

/// GET /v1/cards/lookup/:site_code/:card_code
pub async fn lookup_card(
    State(state): State<AppState>,
    Path((site_code, card_code)): Path<(i32, String)>,
) -> JsonResponse {
    let mut db = state.db.lock().await;

    let sql = format!("{CARD_JOIN_SELECT} WHERE c.iSiteCode = @P1 AND c.iCardCode = @P2");
    let row = match query_opt_row(&mut db, &sql, &[&site_code, &card_code.as_str()]).await {
        Ok(Some(row)) => row,
        Ok(None) => return error_json(StatusCode::NOT_FOUND, "card not found"),
        Err(e) => return internal_error(e),
    };

    ok_json(serde_json::json!({ "ok": true, "card": read_card_with_employee(&row) }))
}

/// DELETE /v1/cards/:id
pub async fn delete_card(State(state): State<AppState>, Path(id): Path<i32>) -> JsonResponse {
    let mut db = state.db.lock().await;

    if let Some(err) = begin(&mut db).await {
        return err;
    }

    let row = match query_opt_row(
        &mut db,
        "SELECT eCardStatus, IdEmpNum FROM tblCard WHERE IdCardNum = @P1",
        &[&id],
    )
    .await
    {
        Ok(Some(row)) => row,
        Ok(None) => {
            rollback(&mut db).await;
            return error_json(StatusCode::NOT_FOUND, "card not found");
        }
        Err(e) => {
            rollback(&mut db).await;
            return internal_error(e);
        }
    };

    let emp_id: i32 = row.get("IdEmpNum").unwrap_or(0);
    if emp_id != 0 {
        rollback(&mut db).await;
        return error_json(
            StatusCode::CONFLICT,
            "card is still assigned to an employee; unassign first",
        );
    }

    if let Err(e) = db
        .execute("DELETE FROM tblSlotCard WHERE IdCardNum = @P1", &[&id])
        .await
    {
        rollback(&mut db).await;
        return internal_error(e);
    }

    if let Err(e) = db
        .execute("DELETE FROM tblCard WHERE IdCardNum = @P1", &[&id])
        .await
    {
        rollback(&mut db).await;
        return internal_error(e);
    }

    if let Some(err) = commit(&mut db).await {
        return err;
    }
    info!("deleted card {id}");
    ok_json(serde_json::json!({ "ok": true }))
}

// ─── Assign / Unassign ───

/// POST /v1/cards/:id/assign
pub async fn assign_card(
    State(state): State<AppState>,
    Path(card_id): Path<i32>,
    Json(req): Json<AssignCardRequest>,
) -> JsonResponse {
    let emp_id = req.employee_id;
    let mut db = state.db.lock().await;

    if let Some(err) = begin(&mut db).await {
        return err;
    }

    // Verify employee exists.
    let emp_exists = match query_exists(
        &mut db,
        "SELECT 1 FROM tblEmployees WHERE iEmployeeNum = @P1",
        &[&emp_id],
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            rollback(&mut db).await;
            return internal_error(e);
        }
    };
    if !emp_exists {
        rollback(&mut db).await;
        return error_json(StatusCode::NOT_FOUND, "employee not found");
    }

    // Verify card exists and is unassigned.
    let card_row = match query_opt_row(
        &mut db,
        "SELECT IdEmpNum FROM tblCard WHERE IdCardNum = @P1",
        &[&card_id],
    )
    .await
    {
        Ok(Some(row)) => row,
        Ok(None) => {
            rollback(&mut db).await;
            return error_json(StatusCode::NOT_FOUND, "card not found");
        }
        Err(e) => {
            rollback(&mut db).await;
            return internal_error(e);
        }
    };

    let current_emp: i32 = card_row.get("IdEmpNum").unwrap_or(0);
    if current_emp != 0 {
        rollback(&mut db).await;
        return error_json(StatusCode::CONFLICT, "card is already assigned");
    }

    // Allocate user slot if employee doesn't have one yet.
    let has_user_slot = match query_opt_row(
        &mut db,
        "SELECT IdUserSlot FROM tblSlotUser WHERE IdUserNum = @P1 AND IdPanel = 1",
        &[&emp_id],
    )
    .await
    {
        Ok(opt) => opt,
        Err(e) => {
            rollback(&mut db).await;
            return internal_error(e);
        }
    };

    if has_user_slot.is_none() {
        let user_slot: i32 = match query_opt_row(
            &mut db,
            "SELECT ISNULL(MAX(IdUserSlot), 0) + 1 FROM tblSlotUser WITH (UPDLOCK, HOLDLOCK) WHERE IdPanel = 1",
            &[],
        ).await {
            Ok(Some(row)) => row.get(0).unwrap_or(1),
            Ok(None) => {
                rollback(&mut db).await;
                return internal_error("failed to allocate user slot");
            }
            Err(e) => {
                rollback(&mut db).await;
                return internal_error(e);
            }
        };

        if let Err(e) = db
            .execute(
                "INSERT INTO tblSlotUser (IdUserNum, IdUserSlot, IdPanel) VALUES (@P1, @P2, 1)",
                &[&emp_id, &user_slot],
            )
            .await
        {
            rollback(&mut db).await;
            return internal_error(e);
        }

        if let Err(e) = db
            .execute(
                "INSERT INTO tblDownload (dtDate, IdNetwork, IdPanel, iPriority, eCommand, IdRecord, iTemp1)
                 VALUES (GETDATE(), 1, 1, 140, 52, @P1, @P2)",
                &[&emp_id, &user_slot],
            )
            .await
        {
            rollback(&mut db).await;
            return internal_error(e);
        }
        info!("allocated user slot {user_slot} for employee {emp_id}");
    }

    // Allocate card slot if card doesn't have one yet.
    let existing_card_slot = match query_opt_row(
        &mut db,
        "SELECT IdCardSlot FROM tblSlotCard WHERE IdCardNum = @P1 AND IdPanel = 1",
        &[&card_id],
    )
    .await
    {
        Ok(opt) => opt,
        Err(e) => {
            rollback(&mut db).await;
            return internal_error(e);
        }
    };

    let card_slot: i32 = if let Some(row) = existing_card_slot {
        row.get("IdCardSlot").unwrap_or(0)
    } else {
        let slot: i32 = match query_opt_row(
            &mut db,
            "SELECT ISNULL(MAX(IdCardSlot), 0) + 1 FROM tblSlotCard WITH (UPDLOCK, HOLDLOCK) WHERE IdPanel = 1",
            &[],
        ).await {
            Ok(Some(row)) => row.get(0).unwrap_or(1),
            Ok(None) => {
                rollback(&mut db).await;
                return internal_error("failed to allocate card slot");
            }
            Err(e) => {
                rollback(&mut db).await;
                return internal_error(e);
            }
        };

        if let Err(e) = db
            .execute(
                "INSERT INTO tblSlotCard (IdCardNum, IdCardSlot, IdPanel) VALUES (@P1, @P2, 1)",
                &[&card_id, &slot],
            )
            .await
        {
            rollback(&mut db).await;
            return internal_error(e);
        }
        info!("allocated card slot {slot} for card {card_id}");
        slot
    };

    // Link card to employee.
    if let Err(e) = db
        .execute(
            "UPDATE tblCard SET IdEmpNum = @P1, eCardStatus = 1 WHERE IdCardNum = @P2",
            &[&emp_id, &card_id],
        )
        .await
    {
        rollback(&mut db).await;
        return internal_error(e);
    }

    // Queue card data push.
    if let Err(e) = db
        .execute(
            "INSERT INTO tblDownload (dtDate, IdNetwork, IdPanel, iPriority, eCommand, IdRecord, iTemp1)
             VALUES (GETDATE(), 1, 1, 145, 50, @P1, @P2)",
            &[&card_id, &card_slot],
        )
        .await
    {
        rollback(&mut db).await;
        return internal_error(e);
    }

    if let Some(err) = commit(&mut db).await {
        return err;
    }
    info!("assigned card {card_id} to employee {emp_id}");
    ok_json(serde_json::json!({ "ok": true }))
}

/// POST /v1/cards/:id/unassign
pub async fn unassign_card(
    State(state): State<AppState>,
    Path(card_id): Path<i32>,
) -> JsonResponse {
    let mut db = state.db.lock().await;

    if let Some(err) = begin(&mut db).await {
        return err;
    }

    // Verify card exists and is assigned.
    let card_row = match query_opt_row(
        &mut db,
        "SELECT IdEmpNum, eCardStatus FROM tblCard WHERE IdCardNum = @P1",
        &[&card_id],
    )
    .await
    {
        Ok(Some(row)) => row,
        Ok(None) => {
            rollback(&mut db).await;
            return error_json(StatusCode::NOT_FOUND, "card not found");
        }
        Err(e) => {
            rollback(&mut db).await;
            return internal_error(e);
        }
    };

    let current_emp: i32 = card_row.get("IdEmpNum").unwrap_or(0);
    if current_emp == 0 {
        rollback(&mut db).await;
        return error_json(StatusCode::CONFLICT, "card is not assigned");
    }

    // Get card slot.
    let card_slot: i32 = match query_opt_row(
        &mut db,
        "SELECT IdCardSlot FROM tblSlotCard WHERE IdCardNum = @P1 AND IdPanel = 1",
        &[&card_id],
    )
    .await
    {
        Ok(Some(row)) => row.get("IdCardSlot").unwrap_or(0),
        Ok(None) => {
            rollback(&mut db).await;
            return internal_error("card has no panel slot");
        }
        Err(e) => {
            rollback(&mut db).await;
            return internal_error(e);
        }
    };

    // Unlink card.
    if let Err(e) = db
        .execute(
            "UPDATE tblCard SET IdEmpNum = 0, eCardStatus = 0 WHERE IdCardNum = @P1",
            &[&card_id],
        )
        .await
    {
        rollback(&mut db).await;
        return internal_error(e);
    }

    // Queue card deletion on panel.
    if let Err(e) = db
        .execute(
            "INSERT INTO tblDownload (dtDate, IdNetwork, IdPanel, iPriority, eCommand, IdRecord, iTemp1)
             VALUES (GETDATE(), 1, 1, 145, 242, @P1, @P2)",
            &[&card_id, &card_slot],
        )
        .await
    {
        rollback(&mut db).await;
        return internal_error(e);
    }

    if let Some(err) = commit(&mut db).await {
        return err;
    }
    info!("unassigned card {card_id} from employee {current_emp}");
    ok_json(serde_json::json!({ "ok": true }))
}
