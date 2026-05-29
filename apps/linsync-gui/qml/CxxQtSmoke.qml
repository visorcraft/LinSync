import QtQml
import com.visorcraft.LinSync

QtObject {
    property CxxQtSmoke smoke: CxxQtSmoke {
        Component.onCompleted: bump()
    }
}
