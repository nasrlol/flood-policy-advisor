//! Closed policy catalogue. Every recommendation the API returns must be one of these ids.
//! Sources: team docs "Policy Brief" (PB) and "Policies since the flood 2021" (P21).

use serde::Serialize;

#[derive(Serialize, Clone, Copy)]
pub struct Policy {
    pub id: &'static str,
    pub horizon: &'static str, // "short" | "mid_long"
    pub pillar: &'static str,
    pub title: &'static str,
    pub what: &'static str,
    pub lesson_2021: &'static str,
    pub source: &'static str,
}

pub const POLICIES: &[Policy] = &[
    Policy {
        id: "ST-ALERT",
        horizon: "short",
        pillar: "Anticipation of crises",
        title: "Early alert system",
        what: "Identify risks and impact areas, monitor flood risk, and issue warnings through several channels (cell broadcast, sirens, door-to-door) so they also reach people without a phone or asleep at night.",
        lesson_2021: "No alert reached Verviers authorities before 14 July; only 17.5% of affected Ahr residents got an evacuation instruction; BE-Alert misses people with no or switched-off phones, and the 2021 peak hit many valleys at night.",
        source: "PB Policy implications / Short-term; P21 sections 1 and 'Weak points'",
    },
    Policy {
        id: "ST-PREP",
        horizon: "short",
        pillar: "Anticipation of crises",
        title: "Preparedness and critical infrastructure",
        what: "Protect critical infrastructure (hospitals, power, drinking water, telecoms) and line up alternative supplies and reinforcements (civil protection, army, boats, helicopters) before the peak.",
        lesson_2021: "Responders in Limbourg (BE) lost power and communication; Eschweiler hospital lost water and power and ICU patients were flown off the roof; Pepinster lacked reinforcements and a rescue boat capsized.",
        source: "PB Short-term; P21 sections 3 and 4",
    },
    Policy {
        id: "ST-AI",
        horizon: "short",
        pillar: "Anticipation of crises",
        title: "Responsible AI for detection, monitoring and against fake news",
        what: "Use AI models for early detection and monitoring of flood risk and real-time information on evacuation and shelter capacity, with human accountability for each decision, and detect misinformation during the crisis.",
        lesson_2021: "Hazard maps underestimated the risk: 58% of fatal incidents happened outside mapped flood zones.",
        source: "PB Takeaways and Short-term",
    },
    Policy {
        id: "ST-CRISIS",
        horizon: "short",
        pillar: "Coordination of responses",
        title: "Coordinated crisis management",
        what: "Crisis response led by public authorities with private companies and households, with clear roles and aligned warning levels across municipalities and borders.",
        lesson_2021: "The evaluation described a stretched crisis organisation where officials wore several hats; warning levels and colour codes still differ between and within countries.",
        source: "PB Short-term; P21 sections 2 and 'Weak points'",
    },
    Policy {
        id: "ST-MITIG",
        horizon: "short",
        pillar: "Coordination of responses",
        title: "Risk mitigation in planning, insurance and budgets",
        what: "Integrate flood risk into urban and rural planning, insurance policies, public finance and budget planning.",
        lesson_2021: "Most damaged homes were rebuilt in the same places, so the number of exposed buildings is largely unchanged.",
        source: "PB Short-term; P21 'How the region would do'",
    },
    Policy {
        id: "ST-COMMS",
        horizon: "short",
        pillar: "Coordination of responses",
        title: "Public communication on what to do",
        what: "Tell residents what to do in each scenario: avoid risky areas, stay at home or leave, put waterproof barriers in front of doors.",
        lesson_2021: "84% of residents were surprised by the size of the flood; most learned they had to leave when the water was already there.",
        source: "PB Short-term; P21 section 1",
    },
    Policy {
        id: "ST-INCLUSIVE",
        horizon: "short",
        pillar: "Coordination of responses",
        title: "Inclusive crisis management (Help a Friend, barrier-free shelters)",
        what: "Help people with reduced mobility and older people reach safe shelters, e.g. through the Help a Friend volunteer app, and build barrier-free shelters.",
        lesson_2021: "In North Rhine-Westphalia people over 60 made up two thirds of the dead (Thieken et al. 2023).",
        source: "PB Short-term",
    },
    Policy {
        id: "ST-CAREHOMES",
        horizon: "short",
        pillar: "Coordination of responses",
        title: "Early evacuation of care homes and hospitals",
        what: "Evacuate care home residents and hospital patients early and on time, before roads close.",
        lesson_2021: "Valkenburg nursing homes could hardly be evacuated as the water rose; Eschweiler ICU was evacuated by helicopter after being told evacuation was not needed.",
        source: "PB Takeaways and Short-term; P21 section 5",
    },
    Policy {
        id: "ST-EVACAPP",
        horizon: "short",
        pillar: "Coordination of responses",
        title: "Interactive evacuation app",
        what: "Give each resident the fastest evacuation route, let users report blocked roads, and show the capacity and occupancy of each shelter.",
        lesson_2021: "Flash floods in small valleys (Vesdre, Geul, Inde) give very little warning, so plans must fit a short window; preventive evacuation along the Meuse (12,000 people in South Limburg) worked where water rose slowly.",
        source: "PB Short-term; P21 'What worked better'",
    },
    Policy {
        id: "LT-ZONING",
        horizon: "mid_long",
        pillar: "Institutionalization of crisis management",
        title: "Pre-crisis relocation under a new zoning system",
        what: "Update hazard zoning and relocate or stop rebuilding where the risk stays too high.",
        lesson_2021: "Dolhain halted repairs on 42 social housing units over fear of future floods; 58% of fatal incidents were outside mapped hazard zones.",
        source: "PB Mid-to-long term; P21 Province of Liège",
    },
    Policy {
        id: "LT-RECOVERY",
        horizon: "mid_long",
        pillar: "Institutionalization of crisis management",
        title: "Post-crisis recovery plans",
        what: "Prepare recovery plans (compensation, reconstruction, restoring defences) before the next event.",
        lesson_2021: "98.3% of claims were paid five years on, but not all infrastructure is restored; a Chaudfontaine official said protection is sometimes worse than before.",
        source: "PB Mid-to-long term; P21 Province of Liège",
    },
    Policy {
        id: "LT-AWARE",
        horizon: "mid_long",
        pillar: "Institutionalization of crisis management",
        title: "Awareness through education and communication",
        what: "Raise public awareness before and after crises through schools, campaigns and drills.",
        lesson_2021: "84% of residents were surprised by the magnitude of the flood.",
        source: "PB Takeaways and Mid-to-long term",
    },
    Policy {
        id: "LT-SEWAGE",
        horizon: "mid_long",
        pillar: "Institutionalization of crisis management",
        title: "Modernise the sewage system (if budget allows)",
        what: "Upgrade the old sewage and drainage system for heavy rain; disconnect downpipes from sewers and retain water at the source.",
        lesson_2021: "Deltares recommended retaining water at the source, less paved surface and disconnecting downpipes from sewers.",
        source: "PB Takeaways and Mid-to-long term; P21 Dutch Limburg",
    },
    Policy {
        id: "LT-RETENTION",
        horizon: "mid_long",
        pillar: "Institutionalization of crisis management",
        title: "Basin-level, cross-border water retention",
        what: "Plan per river basin across borders: retention basins upstream, more nature and less paved surface.",
        lesson_2021: "Retention basins upstream of Stolberg are planned by 2027; the Schéma Vesdre covers 25 municipalities; most Geul water came from across the border. A 2021-type event (~1,000-year) cannot be fully stopped.",
        source: "P21 'What has been done' (Aachen region, Liège, Dutch Limburg)",
    },
];

pub fn get(id: &str) -> Option<&'static Policy> {
    POLICIES.iter().find(|p| p.id == id)
}

pub fn ids() -> Vec<&'static str> {
    POLICIES.iter().map(|p| p.id).collect()
}
