import com.android.build.gradle.BaseExtension

allprojects {
    repositories {
        google()
        mavenCentral()
    }
}

val newBuildDir: Directory =
    rootProject.layout.buildDirectory
        .dir("../../build")
        .get()
rootProject.layout.buildDirectory.value(newBuildDir)

subprojects {
    val newSubprojectBuildDir: Directory = newBuildDir.dir(project.name)
    project.layout.buildDirectory.value(newSubprojectBuildDir)
}

// Register the compileSdk override BEFORE the evaluationDependsOn block
// below. That block forces evaluation of :app early, which cascades into
// the subprojects; afterEvaluate callbacks registered after it has run are
// rejected by Gradle.
subprojects {
    afterEvaluate {
        val androidExt = extensions.findByName("android") as? BaseExtension
            ?: return@afterEvaluate
        val current = androidExt.compileSdkVersion
        val target = 36
        if (current != null &&
            current.removePrefix("android-").toIntOrNull()?.let { it < target } == true
        ) {
            logger.lifecycle(
                "Overriding compileSdk for ${project.name} from $current to android-$target"
            )
            androidExt.compileSdkVersion(target)
        }
    }
}

subprojects {
    project.evaluationDependsOn(":app")
}

tasks.register<Delete>("clean") {
    delete(rootProject.layout.buildDirectory)
}
