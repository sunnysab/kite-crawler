use crate::communication::{AgentData, Response, ResponsePayload};
use crate::make_parameter;
use crate::net::{domain, Client, ClientBuilder, Session};
use crate::parser::{CourseScore, Parse};
use reqwest::{Response as HttpResponse, StatusCode};
use serde::Deserialize;

const INDEX_PAGE: &str = "http://ems1.sit.edu.cn:85/student/";

#[derive(Deserialize)]
pub struct CourseScoreRequest {
    pub account: String,
    pub credential: String,
    pub term: String,
}

impl CourseScoreRequest {
    /// Convert term string format.
    /// 2020A -> 2020春, 2020B -> 2020秋
    fn get_term_string(origin: String) -> String {
        origin.replace("A", "%B4%BA").replace("B", "%C7%EF")
    }

    async fn get_with_auto_redirect(client: &mut Client, start_page: &str) -> HttpResponse {
        let mut remain_redirect = 10;
        let mut next_hop = start_page.to_string();
        let mut response = client.get(&next_hop).send().await.unwrap();

        while remain_redirect > 0 && response.status() == StatusCode::FOUND {
            let redirect_url = response.headers().get("location");
            if redirect_url.is_none() {
                return response;
            }
            let t = redirect_url.unwrap().to_str().unwrap().to_string();
            next_hop = if domain(&t).is_none() {
                format!("http://{}/{}", domain(&next_hop).unwrap(), t)
            } else {
                t
            };
            response = client.get(&next_hop).send().await.unwrap();
            remain_redirect -= 1;
        }
        response
    }

    pub async fn process(self, parameter: AgentData) -> Response {
        let mut session_storage = parameter.parameter;
        // Get account session. If account in storage but the password is out of date, update it. When the session query
        // failed, add the account to session storage.
        let session = if let Some(mut s) = session_storage.query(&self.account).unwrap() {
            if s.password != self.credential {
                s.password = self.credential;
            }
            s
        } else {
            let mut s = Session::new(&self.account, &self.credential);
            if s.login().await.is_err() {
                panic!("密码错误");
            }
            s
        };

        let mut client = ClientBuilder::new(session).redirect(false).build();

        // When we access ems.sit.edu.cn for the first time, the host will set cookies in sub-domain.
        let auth = client.session().query_cookie("ems.sit.edu.cn", "EMS_TOKEN");

        if auth.is_none() {
            Self::get_with_auto_redirect(&mut client, "http://ems1.sit.edu.cn:85/student/").await;
        }

        let response = client
            .post("http://ems1.sit.edu.cn:85/student/graduate/scorelist.jsp")
            // .header("content-type", "application/x-www-form-urlencoded")
            .text(
                (&make_parameter!(
                    "yearterm" => &Self::get_term_string(self.term),
                    "studentID" => &self.account
                ))
                    .as_ref(),
            )
            .send()
            .await
            .unwrap();
        session_storage.insert(client.session());

        let html = response.text_with_charset("GBK").await.unwrap();
        let course_scores: Vec<CourseScore> = Parse::from_html(&html);
        println!("{:#?}", course_scores);
        Response::normal(ResponsePayload::ScoreList(course_scores))
    }
}