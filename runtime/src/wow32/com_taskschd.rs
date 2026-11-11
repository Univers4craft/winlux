//! Planificateur de tâches 2.0 pour les programmes x86 (#1757).
//!
//! Description des interfaces de `taskschd.h` (en-têtes publics du SDK
//! Windows, aussi dans mingw-w64) pour les mandataires COM de `com.rs` :
//! sans elles, `CoCreateInstance(CLSID_TaskScheduler, IID_ITaskService)`
//! rendait E_NOINTERFACE à une action personnalisée x86 (AddTask
//! d'Acrobat). Les objets eux-mêmes sont ceux de `dlls/taskschd.rs`.
//!
//! Conventions : un BSTR passé est un pointeur (`I`), un BSTR rendu
//! (`BSTR*`) une valeur de la taille d'un pointeur (`OutPtr`) ; un
//! VARIANT passé par valeur est `VarIn` ; VARIANT_BOOL*, LONG*, DATE*
//! pointent vers des données sans pointeur (`I`).

use super::com::{Iface, A, A::*, IDISPATCH, IUNKNOWN};
use crate::dlls::comobj::{guid, Guid};

const BS_GET: Option<&[A]> = Some(&[OutPtr]);
const SET: Option<&[A]> = Some(&[I]);
const GET: Option<&[A]> = Some(&[I]);
const NONE: Option<&[A]> = Some(&[]);

const fn iid(d1: u32, d2: u16, d3: u16, d4: [u8; 8]) -> Guid {
    guid(d1, d2, d3, d4)
}

pub(super) static ITASKSERVICE: Iface = Iface {
    name: "ITaskService",
    iid: iid(
        0x2FAB_A4C7,
        0x4DA9,
        0x4013,
        [0x96, 0x97, 0x20, 0xCC, 0x3F, 0xD4, 0x0F, 0x85],
    ),
    parent: Some(&IDISPATCH),
    methods: &[
        Some(&[I, Out(&ITASKFOLDER)]),            // GetFolder
        Some(&[I, Out(&IRUNNINGTASKCOLLECTION)]), // GetRunningTasks
        Some(&[I, Out(&ITASKDEFINITION)]),        // NewTask
        Some(&[VarIn, VarIn, VarIn, VarIn]),      // Connect
        GET,                                      // get_Connected
        BS_GET,                                   // get_TargetServer
        BS_GET,                                   // get_ConnectedUser
        BS_GET,                                   // get_ConnectedDomain
        GET,                                      // get_HighestVersion
    ],
};

pub(super) static ITASKFOLDER: Iface = Iface {
    name: "ITaskFolder",
    iid: iid(
        0x8CFA_C062,
        0xA080,
        0x4C15,
        [0x9A, 0x88, 0xAA, 0x7C, 0x2A, 0xF8, 0x0D, 0xFC],
    ),
    parent: Some(&IDISPATCH),
    methods: &[
        BS_GET,                                                          // get_Name
        BS_GET,                                                          // get_Path
        Some(&[I, Out(&ITASKFOLDER)]),                                   // GetFolder
        Some(&[I, Out(&ITASKFOLDERCOLLECTION)]),                         // GetFolders
        Some(&[I, VarIn, Out(&ITASKFOLDER)]),                            // CreateFolder
        Some(&[I, I]),                                                   // DeleteFolder
        Some(&[I, Out(&IREGISTEREDTASK)]),                               // GetTask
        Some(&[I, Out(&IREGISTEREDTASKCOLLECTION)]),                     // GetTasks
        Some(&[I, I]),                                                   // DeleteTask
        Some(&[I, I, I, VarIn, VarIn, I, VarIn, Out(&IREGISTEREDTASK)]), // RegisterTask
        Some(&[
            I,
            In(&ITASKDEFINITION),
            I,
            VarIn,
            VarIn,
            I,
            VarIn,
            Out(&IREGISTEREDTASK),
        ]), // RegisterTaskDefinition
        Some(&[I, OutPtr]),                                              // GetSecurityDescriptor
        Some(&[I, I]),                                                   // SetSecurityDescriptor
    ],
};

pub(super) static ITASKFOLDERCOLLECTION: Iface = Iface {
    name: "ITaskFolderCollection",
    iid: iid(
        0x7918_4A66,
        0x8664,
        0x423F,
        [0x97, 0xF1, 0x63, 0x73, 0x56, 0xA5, 0xD8, 0x12],
    ),
    parent: Some(&IDISPATCH),
    methods: &[
        GET,                               // get_Count
        Some(&[VarIn, Out(&ITASKFOLDER)]), // get_Item
        Some(&[Out(&IUNKNOWN)]),           // get__NewEnum
    ],
};

pub(super) static IREGISTEREDTASK: Iface = Iface {
    name: "IRegisteredTask",
    iid: iid(
        0x9C86_F320,
        0xDEE3,
        0x4DD1,
        [0xB9, 0x72, 0xA3, 0x03, 0xF2, 0x6B, 0x06, 0x1E],
    ),
    parent: Some(&IDISPATCH),
    methods: &[
        BS_GET,                                      // get_Name
        BS_GET,                                      // get_Path
        GET,                                         // get_State
        GET,                                         // get_Enabled
        SET,                                         // put_Enabled
        Some(&[VarIn, Out(&IRUNNINGTASK)]),          // Run
        Some(&[VarIn, I, I, I, Out(&IRUNNINGTASK)]), // RunEx
        Some(&[I, Out(&IRUNNINGTASKCOLLECTION)]),    // GetInstances
        GET,                                         // get_LastRunTime
        GET,                                         // get_LastTaskResult
        GET,                                         // get_NumberOfMissedRuns
        GET,                                         // get_NextRunTime
        Some(&[Out(&ITASKDEFINITION)]),              // get_Definition
        BS_GET,                                      // get_Xml
        Some(&[I, OutPtr]),                          // GetSecurityDescriptor
        Some(&[I, I]),                               // SetSecurityDescriptor
        SET,                                         // Stop
        None,                                        // GetRunTimes
    ],
};

pub(super) static IREGISTEREDTASKCOLLECTION: Iface = Iface {
    name: "IRegisteredTaskCollection",
    iid: iid(
        0x8662_7EB4,
        0x42A7,
        0x41E4,
        [0xA4, 0xD9, 0xAC, 0x33, 0xA7, 0x2F, 0x2D, 0x52],
    ),
    parent: Some(&IDISPATCH),
    methods: &[
        GET,                                   // get_Count
        Some(&[VarIn, Out(&IREGISTEREDTASK)]), // get_Item
        Some(&[Out(&IUNKNOWN)]),               // get__NewEnum
    ],
};

pub(super) static IRUNNINGTASK: Iface = Iface {
    name: "IRunningTask",
    iid: iid(
        0x6537_58FB,
        0x7B9A,
        0x4F1E,
        [0xA4, 0x71, 0xBE, 0xEB, 0x8E, 0x9B, 0x83, 0x4E],
    ),
    parent: Some(&IDISPATCH),
    methods: &[
        BS_GET, // get_Name
        BS_GET, // get_InstanceGuid
        BS_GET, // get_Path
        GET,    // get_State
        BS_GET, // get_CurrentAction
        NONE,   // Stop
        NONE,   // Refresh
        GET,    // get_EnginePID
    ],
};

pub(super) static IRUNNINGTASKCOLLECTION: Iface = Iface {
    name: "IRunningTaskCollection",
    iid: iid(
        0x6A67_614B,
        0x6828,
        0x4FEC,
        [0xAA, 0x54, 0x6D, 0x52, 0xE8, 0xF1, 0xF2, 0xDB],
    ),
    parent: Some(&IDISPATCH),
    methods: &[
        GET,                                // get_Count
        Some(&[VarIn, Out(&IRUNNINGTASK)]), // get_Item
        Some(&[Out(&IUNKNOWN)]),            // get__NewEnum
    ],
};

pub(super) static ITASKDEFINITION: Iface = Iface {
    name: "ITaskDefinition",
    iid: iid(
        0xF5BC_8FC5,
        0x536D,
        0x4F77,
        [0xB8, 0x52, 0xFB, 0xC1, 0x35, 0x6F, 0xDE, 0xB6],
    ),
    parent: Some(&IDISPATCH),
    methods: &[
        Some(&[Out(&IREGISTRATIONINFO)]),  // get_RegistrationInfo
        Some(&[In(&IREGISTRATIONINFO)]),   // put_RegistrationInfo
        Some(&[Out(&ITRIGGERCOLLECTION)]), // get_Triggers
        Some(&[In(&ITRIGGERCOLLECTION)]),  // put_Triggers
        Some(&[Out(&ITASKSETTINGS)]),      // get_Settings
        Some(&[In(&ITASKSETTINGS)]),       // put_Settings
        BS_GET,                            // get_Data
        SET,                               // put_Data
        Some(&[Out(&IPRINCIPAL)]),         // get_Principal
        Some(&[In(&IPRINCIPAL)]),          // put_Principal
        Some(&[Out(&IACTIONCOLLECTION)]),  // get_Actions
        Some(&[In(&IACTIONCOLLECTION)]),   // put_Actions
        BS_GET,                            // get_XmlText
        SET,                               // put_XmlText
    ],
};

pub(super) static IREGISTRATIONINFO: Iface = Iface {
    name: "IRegistrationInfo",
    iid: iid(
        0x416D_8B73,
        0xCB41,
        0x4EA1,
        [0x80, 0x5C, 0x9B, 0xE9, 0xA5, 0xAC, 0x4A, 0x74],
    ),
    parent: Some(&IDISPATCH),
    methods: &[
        BS_GET,         // get_Description
        SET,            // put_Description
        BS_GET,         // get_Author
        SET,            // put_Author
        BS_GET,         // get_Version
        SET,            // put_Version
        BS_GET,         // get_Date
        SET,            // put_Date
        BS_GET,         // get_Documentation
        SET,            // put_Documentation
        BS_GET,         // get_XmlText
        SET,            // put_XmlText
        BS_GET,         // get_URI
        SET,            // put_URI
        Some(&[Var]),   // get_SecurityDescriptor
        Some(&[VarIn]), // put_SecurityDescriptor
        BS_GET,         // get_Source
        SET,            // put_Source
    ],
};

pub(super) static ITASKSETTINGS: Iface = Iface {
    name: "ITaskSettings",
    iid: iid(
        0x8FD4_711D,
        0x2D02,
        0x4C8C,
        [0x87, 0xE3, 0xEF, 0xF6, 0x99, 0xDE, 0x12, 0x7E],
    ),
    parent: Some(&IDISPATCH),
    methods: &[
        GET,                     // get_AllowDemandStart
        SET,                     // put_AllowDemandStart
        BS_GET,                  // get_RestartInterval
        SET,                     // put_RestartInterval
        GET,                     // get_RestartCount
        SET,                     // put_RestartCount
        GET,                     // get_MultipleInstances
        SET,                     // put_MultipleInstances
        GET,                     // get_StopIfGoingOnBatteries
        SET,                     // put_StopIfGoingOnBatteries
        GET,                     // get_DisallowStartIfOnBatteries
        SET,                     // put_DisallowStartIfOnBatteries
        GET,                     // get_AllowHardTerminate
        SET,                     // put_AllowHardTerminate
        GET,                     // get_StartWhenAvailable
        SET,                     // put_StartWhenAvailable
        BS_GET,                  // get_XmlText
        SET,                     // put_XmlText
        GET,                     // get_RunOnlyIfNetworkAvailable
        SET,                     // put_RunOnlyIfNetworkAvailable
        BS_GET,                  // get_ExecutionTimeLimit
        SET,                     // put_ExecutionTimeLimit
        GET,                     // get_Enabled
        SET,                     // put_Enabled
        BS_GET,                  // get_DeleteExpiredTaskAfter
        SET,                     // put_DeleteExpiredTaskAfter
        GET,                     // get_Priority
        SET,                     // put_Priority
        GET,                     // get_Compatibility
        SET,                     // put_Compatibility
        GET,                     // get_Hidden
        SET,                     // put_Hidden
        Some(&[Out(&IUNKNOWN)]), // get_IdleSettings (aucun objet rendu)
        SET,                     // put_IdleSettings (ignoré)
        GET,                     // get_RunOnlyIfIdle
        SET,                     // put_RunOnlyIfIdle
        GET,                     // get_WakeToRun
        SET,                     // put_WakeToRun
        Some(&[Out(&IUNKNOWN)]), // get_NetworkSettings (aucun objet rendu)
        SET,                     // put_NetworkSettings (ignoré)
    ],
};

pub(super) static IPRINCIPAL: Iface = Iface {
    name: "IPrincipal",
    iid: iid(
        0xD98D_51E5,
        0xC9B4,
        0x496A,
        [0xA9, 0xC1, 0x18, 0x98, 0x02, 0x61, 0xCF, 0x0F],
    ),
    parent: Some(&IDISPATCH),
    methods: &[
        BS_GET, // get_Id
        SET,    // put_Id
        BS_GET, // get_DisplayName
        SET,    // put_DisplayName
        BS_GET, // get_UserId
        SET,    // put_UserId
        GET,    // get_LogonType
        SET,    // put_LogonType
        BS_GET, // get_GroupId
        SET,    // put_GroupId
        GET,    // get_RunLevel
        SET,    // put_RunLevel
    ],
};

pub(super) static ITRIGGERCOLLECTION: Iface = Iface {
    name: "ITriggerCollection",
    iid: iid(
        0x85DF_5081,
        0x1B24,
        0x4F32,
        [0x87, 0x8A, 0xD9, 0xD1, 0x4D, 0xF4, 0xCB, 0x77],
    ),
    parent: Some(&IDISPATCH),
    methods: &[
        GET,                        // get_Count
        Some(&[I, Out(&ITRIGGER)]), // get_Item
        Some(&[Out(&IUNKNOWN)]),    // get__NewEnum
        Some(&[I, Out(&ITRIGGER)]), // Create
        Some(&[VarIn]),             // Remove
        NONE,                       // Clear
    ],
};

/// Méthodes communes d'ITrigger, reprises par chaque type de déclencheur.
const TRIGGER_METHODS: [Option<&[A]>; 13] = [
    GET,                               // get_Type
    BS_GET,                            // get_Id
    SET,                               // put_Id
    Some(&[Out(&IREPETITIONPATTERN)]), // get_Repetition
    Some(&[In(&IREPETITIONPATTERN)]),  // put_Repetition
    BS_GET,                            // get_ExecutionTimeLimit
    SET,                               // put_ExecutionTimeLimit
    BS_GET,                            // get_StartBoundary
    SET,                               // put_StartBoundary
    BS_GET,                            // get_EndBoundary
    SET,                               // put_EndBoundary
    GET,                               // get_Enabled
    SET,                               // put_Enabled
];

pub(super) static ITRIGGER: Iface = Iface {
    name: "ITrigger",
    iid: iid(
        0x0994_1815,
        0xEA89,
        0x4B5B,
        [0x89, 0xE0, 0x2A, 0x77, 0x38, 0x01, 0xFA, 0xC3],
    ),
    parent: Some(&IDISPATCH),
    methods: &TRIGGER_METHODS,
};

pub(super) static ITIMETRIGGER: Iface = Iface {
    name: "ITimeTrigger",
    iid: iid(
        0xB457_47E0,
        0xEBA7,
        0x4276,
        [0x9F, 0x29, 0x85, 0xC5, 0xBB, 0x30, 0x00, 0x06],
    ),
    parent: Some(&ITRIGGER),
    methods: &[BS_GET, SET], // RandomDelay
};

pub(super) static IDAILYTRIGGER: Iface = Iface {
    name: "IDailyTrigger",
    iid: iid(
        0x126C_5CD8,
        0xB288,
        0x41D5,
        [0x8D, 0xBF, 0xE4, 0x91, 0x44, 0x6A, 0xDC, 0x5C],
    ),
    parent: Some(&ITRIGGER),
    methods: &[GET, SET, BS_GET, SET], // DaysInterval, RandomDelay
};

pub(super) static IWEEKLYTRIGGER: Iface = Iface {
    name: "IWeeklyTrigger",
    iid: iid(
        0x5038_FC98,
        0x82FF,
        0x436D,
        [0x87, 0x28, 0xA5, 0x12, 0xA5, 0x7C, 0x9D, 0xC1],
    ),
    parent: Some(&ITRIGGER),
    // DaysOfWeek, WeeksInterval, RandomDelay
    methods: &[GET, SET, GET, SET, BS_GET, SET],
};

pub(super) static IBOOTTRIGGER: Iface = Iface {
    name: "IBootTrigger",
    iid: iid(
        0x2A9C_35DA,
        0xD357,
        0x41F4,
        [0xBB, 0xC1, 0x20, 0x7A, 0xC1, 0xB1, 0xF3, 0xCB],
    ),
    parent: Some(&ITRIGGER),
    methods: &[BS_GET, SET], // Delay
};

pub(super) static ILOGONTRIGGER: Iface = Iface {
    name: "ILogonTrigger",
    iid: iid(
        0x72DA_DE38,
        0xFAE4,
        0x4B3E,
        [0xBA, 0xF4, 0x5D, 0x00, 0x9A, 0xF0, 0x2B, 0x1C],
    ),
    parent: Some(&ITRIGGER),
    methods: &[BS_GET, SET, BS_GET, SET], // Delay, UserId
};

pub(super) static IEVENTTRIGGER: Iface = Iface {
    name: "IEventTrigger",
    iid: iid(
        0xD45B_0167,
        0x9653,
        0x4EEF,
        [0xB9, 0x4F, 0x07, 0x32, 0xCA, 0x7A, 0xF2, 0x51],
    ),
    parent: Some(&ITRIGGER),
    // Subscription, Delay, ValueQueries (aucun objet rendu, affectation ignorée)
    methods: &[BS_GET, SET, BS_GET, SET, Some(&[Out(&IUNKNOWN)]), SET],
};

pub(super) static IREGISTRATIONTRIGGER: Iface = Iface {
    name: "IRegistrationTrigger",
    iid: iid(
        0x4C8F_EC3A,
        0xC218,
        0x4E0C,
        [0xB2, 0x3D, 0x62, 0x90, 0x24, 0xDB, 0x91, 0xA2],
    ),
    parent: Some(&ITRIGGER),
    methods: &[BS_GET, SET], // Delay
};

pub(super) static IREPETITIONPATTERN: Iface = Iface {
    name: "IRepetitionPattern",
    iid: iid(
        0x7FB9_ACF1,
        0x26BE,
        0x400E,
        [0x85, 0xB5, 0x29, 0x4B, 0x9C, 0x75, 0xDF, 0xD6],
    ),
    parent: Some(&IDISPATCH),
    // Interval, Duration, StopAtDurationEnd
    methods: &[BS_GET, SET, BS_GET, SET, GET, SET],
};

pub(super) static IACTIONCOLLECTION: Iface = Iface {
    name: "IActionCollection",
    iid: iid(
        0x0282_0E19,
        0x7B98,
        0x4ED2,
        [0xB2, 0xE8, 0xFD, 0xCC, 0xCE, 0xFF, 0x61, 0x9B],
    ),
    parent: Some(&IDISPATCH),
    methods: &[
        GET,                       // get_Count
        Some(&[I, Out(&IACTION)]), // get_Item
        Some(&[Out(&IUNKNOWN)]),   // get__NewEnum
        BS_GET,                    // get_XmlText
        SET,                       // put_XmlText
        Some(&[I, Out(&IACTION)]), // Create
        Some(&[VarIn]),            // Remove
        NONE,                      // Clear
        BS_GET,                    // get_Context
        SET,                       // put_Context
    ],
};

pub(super) static IACTION: Iface = Iface {
    name: "IAction",
    iid: iid(
        0xBAE5_4997,
        0x48B1,
        0x4CBE,
        [0x99, 0x65, 0xD6, 0xBE, 0x26, 0x3E, 0xBE, 0xA4],
    ),
    parent: Some(&IDISPATCH),
    methods: &[BS_GET, SET, GET], // Id, get_Type
};

pub(super) static IEXECACTION: Iface = Iface {
    name: "IExecAction",
    iid: iid(
        0x4C3D_624D,
        0xFD6B,
        0x49A3,
        [0xB9, 0xB7, 0x09, 0xCB, 0x3C, 0xD3, 0xF0, 0x47],
    ),
    parent: Some(&IACTION),
    // Path, Arguments, WorkingDirectory
    methods: &[BS_GET, SET, BS_GET, SET, BS_GET, SET],
};

/// Toutes les interfaces du planificateur, pour `com::by_iid`.
pub(super) static ALL: &[&Iface] = &[
    &ITASKSERVICE,
    &ITASKFOLDER,
    &ITASKFOLDERCOLLECTION,
    &IREGISTEREDTASK,
    &IREGISTEREDTASKCOLLECTION,
    &IRUNNINGTASK,
    &IRUNNINGTASKCOLLECTION,
    &ITASKDEFINITION,
    &IREGISTRATIONINFO,
    &ITASKSETTINGS,
    &IPRINCIPAL,
    &ITRIGGERCOLLECTION,
    &ITRIGGER,
    &ITIMETRIGGER,
    &IDAILYTRIGGER,
    &IWEEKLYTRIGGER,
    &IBOOTTRIGGER,
    &ILOGONTRIGGER,
    &IEVENTTRIGGER,
    &IREGISTRATIONTRIGGER,
    &IREPETITIONPATTERN,
    &IACTIONCOLLECTION,
    &IACTION,
    &IEXECACTION,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planificateur_decrit_pour_le_code_x86() {
        // AddTask d'Acrobat (#1757) : ITaskService et ce qu'il rend sont
        // décrits, avec autant de méthodes que les tables de taskschd.rs.
        for (f, own) in [
            (&ITASKSERVICE, 9),
            (&ITASKFOLDER, 13),
            (&IREGISTEREDTASK, 18),
            (&ITASKDEFINITION, 14),
            (&IREGISTRATIONINFO, 18),
            (&ITASKSETTINGS, 40),
            (&IPRINCIPAL, 12),
            (&ITRIGGERCOLLECTION, 6),
            (&ITRIGGER, 13),
            (&IACTIONCOLLECTION, 10),
            (&IACTION, 3),
            (&IRUNNINGTASK, 8),
        ] {
            assert_eq!(f.methods.len(), own, "{}", f.name);
        }
        assert_eq!(ILOGONTRIGGER.methods.len(), 4);
        assert_eq!(IEXECACTION.methods.len(), 6);
        let mut seen = std::collections::HashSet::new();
        assert!(ALL.iter().all(|f| seen.insert(f.iid)), "IID en double");
    }
}
